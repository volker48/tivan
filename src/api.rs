//! The Worker's HTTP API as the client sees it. [`Http`] is the one thing that touches the
//! network, so everything above it, the status handling and the parsing, runs against a fake
//! under `cargo test` the way [`store`](crate::store) runs against an in-memory `Storage`.
//!
//! A call returns a typed answer or an [`ApiError`] that says what the page should do: sign the
//! teacher out, send them through Google again, or show a cause.

use std::fmt;
use std::future::Future;

use serde::Deserialize;

use crate::domain::{ClassroomCourse, parse_courses};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Put,
    Post,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Put => "PUT",
            Method::Post => "POST",
        }
    }
}

/// A same-origin request. A body is always JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HttpRequest<'a> {
    pub method: Method,
    pub path: &'a str,
    pub body: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpReply {
    pub status: u16,
    pub body: String,
}

/// No reply came back at all: offline, a timeout, or the browser refused the request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportError(String);

impl TransportError {
    pub fn new(message: impl Into<String>) -> TransportError {
        TransportError(message.into())
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TransportError {}

/// Sends one request. The reply of any status is a success here, and the caller reads the status.
pub trait Http {
    fn send(
        &self,
        request: HttpRequest<'_>,
    ) -> impl Future<Output = Result<HttpReply, TransportError>>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApiError {
    /// Not signed in, or the session has ended.
    Unauthorized,
    /// Google no longer honors the teacher's grant, so they must sign in with Google again.
    Reauthorize,
    /// Anything else, as text for a banner: no reply, an error status, or an answer that is not
    /// what the contract promises.
    Failed(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Unauthorized => f.write_str("you are not signed in"),
            ApiError::Reauthorize => f.write_str("Google needs you to sign in again"),
            ApiError::Failed(cause) => f.write_str(cause),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<TransportError> for ApiError {
    fn from(error: TransportError) -> ApiError {
        ApiError::Failed(format!("could not reach the server: {error}"))
    }
}

/// The signed-in teacher, as `GET /api/me` describes them.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Me {
    pub email: String,
    pub name: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

/// Longest `error` code worth showing.
const MAX_CODE_LEN: usize = 40;

/// A short lowercase code such as `reauthorize` or `access_denied`. Anything else, which only a
/// broken or hostile server or link would send, is not shown to the teacher.
pub fn short_code(raw: &str) -> Option<&str> {
    let valid = !raw.is_empty()
        && raw.len() <= MAX_CODE_LEN
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    valid.then_some(raw)
}

fn error_of(reply: &HttpReply) -> ApiError {
    let body = serde_json::from_str::<ErrorBody>(&reply.body).ok();
    let code = body.as_ref().and_then(|body| short_code(&body.error));
    match (reply.status, code) {
        (401, _) => ApiError::Unauthorized,
        (403, Some("reauthorize")) => ApiError::Reauthorize,
        (status, Some(code)) => ApiError::Failed(format!("the server answered {status}, {code}")),
        (status, None) => ApiError::Failed(format!("the server answered {status}")),
    }
}

pub struct Api<H> {
    http: H,
}

impl<H: Http> Api<H> {
    pub fn new(http: H) -> Api<H> {
        Api { http }
    }

    /// Who is signed in, or `None` when nobody is. A host with no account service counts as
    /// nobody: the dev server answers every path with its index page and a static host with a
    /// 404, and neither is a failure worth a banner. Any other 200 that is not a teacher is a
    /// failure, so a broken server cannot switch the app to local-only saving unnoticed.
    pub async fn me(&self) -> Result<Option<Me>, ApiError> {
        let reply = self.get("/api/me").await?;
        match reply.status {
            200 => match serde_json::from_str(&reply.body) {
                Ok(me) => Ok(Some(me)),
                Err(_) if reply.body.trim_start().starts_with('<') => Ok(None),
                Err(error) => Err(ApiError::Failed(format!(
                    "the server sent an account answer Tivan could not read: {error}"
                ))),
            },
            401 | 404 => Ok(None),
            _ => Err(error_of(&reply)),
        }
    }

    /// The stored envelope exactly as the server holds it, or `None` when it holds none.
    pub async fn doc(&self) -> Result<Option<String>, ApiError> {
        let reply = self.get("/api/doc").await?;
        match reply.status {
            200 => Ok(Some(reply.body)),
            204 => Ok(None),
            _ => Err(error_of(&reply)),
        }
    }

    pub async fn put_doc(&self, envelope: &str) -> Result<(), ApiError> {
        let request = HttpRequest {
            method: Method::Put,
            path: "/api/doc",
            body: Some(envelope),
        };
        let reply = self.http.send(request).await?;
        match reply.status {
            200..=299 => Ok(()),
            _ => Err(error_of(&reply)),
        }
    }

    /// The teacher's active courses with their rosters.
    pub async fn courses(&self) -> Result<Vec<ClassroomCourse>, ApiError> {
        let reply = self.get("/api/classroom/courses").await?;
        match reply.status {
            200 => parse_courses(&reply.body).map_err(|error| {
                ApiError::Failed(format!(
                    "the server sent classes Tivan cannot read: {error}"
                ))
            }),
            _ => Err(error_of(&reply)),
        }
    }

    /// Ends the session. [`ApiError::Unauthorized`] means it had already ended.
    pub async fn logout(&self) -> Result<(), ApiError> {
        let request = HttpRequest {
            method: Method::Post,
            path: "/auth/logout",
            body: None,
        };
        let reply = self.http.send(request).await?;
        match reply.status {
            200..=299 => Ok(()),
            _ => Err(error_of(&reply)),
        }
    }

    async fn get(&self, path: &str) -> Result<HttpReply, TransportError> {
        self.http
            .send(HttpRequest {
                method: Method::Get,
                path,
                body: None,
            })
            .await
    }
}

/// A scripted server for tests, shared by the modules that sit on [`Api`].
#[cfg(test)]
pub(crate) mod fake {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::task::{Context, Poll, Waker};

    use super::*;

    type Route = (Method, &'static str);

    /// Answers each route with the reply it was given, 404 for the rest, and remembers what it
    /// was asked.
    #[derive(Default)]
    pub struct FakeHttp {
        replies: HashMap<Route, Result<HttpReply, TransportError>>,
        asked: RefCell<Vec<(Method, String, Option<String>)>>,
    }

    impl FakeHttp {
        pub fn reply(
            mut self,
            method: Method,
            path: &'static str,
            status: u16,
            body: &str,
        ) -> Self {
            let reply = HttpReply {
                status,
                body: body.to_owned(),
            };
            self.replies.insert((method, path), Ok(reply));
            self
        }

        pub fn unreachable(mut self, method: Method, path: &'static str) -> Self {
            let error = TransportError::new("Failed to fetch");
            self.replies.insert((method, path), Err(error));
            self
        }

        pub fn requests(&self) -> Vec<(Method, String, Option<String>)> {
            self.asked.borrow().clone()
        }

        pub fn wrote(&self) -> bool {
            self.asked
                .borrow()
                .iter()
                .any(|(method, ..)| *method != Method::Get)
        }
    }

    impl Http for &FakeHttp {
        async fn send(&self, request: HttpRequest<'_>) -> Result<HttpReply, TransportError> {
            self.asked.borrow_mut().push((
                request.method,
                request.path.to_owned(),
                request.body.map(str::to_owned),
            ));
            match self.replies.get(&(request.method, request.path)) {
                Some(reply) => reply.clone(),
                None => Ok(HttpReply {
                    status: 404,
                    body: r#"{"error":"not_found"}"#.to_owned(),
                }),
            }
        }
    }

    /// Runs a future that never waits, which is all a fake produces.
    pub fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("a fake server answers at once"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{FakeHttp, block_on};
    use super::*;

    const ME: &str = r#"{"email":"ada@school.org","name":"Ada Lovelace"}"#;

    fn api(server: &FakeHttp) -> Api<&FakeHttp> {
        Api::new(server)
    }

    fn me_with(status: u16, body: &str) -> Result<Option<Me>, ApiError> {
        let server = FakeHttp::default().reply(Method::Get, "/api/me", status, body);
        block_on(api(&server).me())
    }

    #[test]
    fn a_signed_in_teacher_is_read_from_the_me_answer() {
        assert_eq!(
            me_with(200, ME),
            Ok(Some(Me {
                email: "ada@school.org".to_owned(),
                name: "Ada Lovelace".to_owned(),
            }))
        );
    }

    #[test]
    fn nobody_is_signed_in_when_the_server_says_so_or_there_is_no_account_service() {
        assert_eq!(me_with(401, r#"{"error":"unauthorized"}"#), Ok(None));
        assert_eq!(me_with(404, ""), Ok(None));
        assert_eq!(me_with(200, "<!DOCTYPE html><html></html>"), Ok(None));
    }

    #[test]
    fn a_200_that_is_neither_a_teacher_nor_a_page_is_a_failure() {
        for body in ["", "{}", r#"{"email":"a@b.c"}"#, "null", "not json"] {
            assert!(
                matches!(me_with(200, body), Err(ApiError::Failed(_))),
                "body {body:?}"
            );
        }
    }

    #[test]
    fn a_server_error_while_asking_who_is_signed_in_is_a_failure_with_its_code() {
        assert_eq!(
            me_with(500, r#"{"error":"db_down"}"#),
            Err(ApiError::Failed(
                "the server answered 500, db_down".to_owned()
            ))
        );
        assert_eq!(
            me_with(503, "upstream"),
            Err(ApiError::Failed("the server answered 503".to_owned()))
        );
    }

    #[test]
    fn no_reply_at_all_is_a_failure_that_says_so() {
        let server = FakeHttp::default().unreachable(Method::Get, "/api/me");
        assert_eq!(
            block_on(api(&server).me()),
            Err(ApiError::Failed(
                "could not reach the server: Failed to fetch".to_owned()
            ))
        );
    }

    #[test]
    fn the_stored_doc_comes_back_verbatim_and_no_doc_is_none() {
        let stored = r#"{"version":5,"doc":{"classes":[],"selected":null,"next_class":0}}"#;
        let server = FakeHttp::default().reply(Method::Get, "/api/doc", 200, stored);
        assert_eq!(block_on(api(&server).doc()), Ok(Some(stored.to_owned())));

        let server = FakeHttp::default().reply(Method::Get, "/api/doc", 204, "");
        assert_eq!(block_on(api(&server).doc()), Ok(None));
    }

    #[test]
    fn reading_the_doc_signed_out_or_failing_is_an_error_not_an_empty_doc() {
        let server = FakeHttp::default().reply(Method::Get, "/api/doc", 401, r#"{"error":"x"}"#);
        assert_eq!(block_on(api(&server).doc()), Err(ApiError::Unauthorized));

        let server = FakeHttp::default().reply(Method::Get, "/api/doc", 500, "");
        assert!(matches!(
            block_on(api(&server).doc()),
            Err(ApiError::Failed(_))
        ));

        let server = FakeHttp::default().unreachable(Method::Get, "/api/doc");
        assert!(matches!(
            block_on(api(&server).doc()),
            Err(ApiError::Failed(_))
        ));
    }

    #[test]
    fn saving_puts_the_envelope_and_reports_each_refusal() {
        let server = FakeHttp::default().reply(Method::Put, "/api/doc", 204, "");
        assert_eq!(block_on(api(&server).put_doc("{\"version\":5}")), Ok(()));
        assert_eq!(
            server.requests(),
            [(
                Method::Put,
                "/api/doc".to_owned(),
                Some("{\"version\":5}".to_owned())
            )]
        );

        for (status, expected) in [
            (401, ApiError::Unauthorized),
            (
                413,
                ApiError::Failed("the server answered 413, too_large".to_owned()),
            ),
            (400, ApiError::Failed("the server answered 400".to_owned())),
        ] {
            let body = if status == 413 {
                r#"{"error":"too_large"}"#
            } else {
                ""
            };
            let server = FakeHttp::default().reply(Method::Put, "/api/doc", status, body);
            assert_eq!(block_on(api(&server).put_doc("{}")), Err(expected));
        }
    }

    #[test]
    fn courses_are_parsed_from_the_workers_answer() {
        let body =
            r#"{"courses":[{"id":"c1","name":"Math","students":[{"id":"u1","name":"Ava"}]}]}"#;
        let server = FakeHttp::default().reply(Method::Get, "/api/classroom/courses", 200, body);

        let courses = block_on(api(&server).courses()).unwrap();

        assert_eq!(courses.len(), 1);
        assert_eq!(courses[0].name().as_str(), "Math");
        assert_eq!(courses[0].students().len(), 1);
    }

    #[test]
    fn google_refusing_the_grant_asks_for_a_new_sign_in_but_other_forbidden_answers_do_not() {
        let courses = |status, body| {
            let server =
                FakeHttp::default().reply(Method::Get, "/api/classroom/courses", status, body);
            block_on(api(&server).courses())
        };

        assert_eq!(
            courses(403, r#"{"error":"reauthorize"}"#).unwrap_err(),
            ApiError::Reauthorize
        );
        assert_eq!(
            courses(403, r#"{"error":"other"}"#).unwrap_err(),
            ApiError::Failed("the server answered 403, other".to_owned())
        );
        assert_eq!(
            courses(401, r#"{"error":"unauthorized"}"#).unwrap_err(),
            ApiError::Unauthorized
        );
        assert_eq!(
            courses(502, r#"{"error":"classroom_unavailable"}"#).unwrap_err(),
            ApiError::Failed("the server answered 502, classroom_unavailable".to_owned())
        );
    }

    #[test]
    fn classes_in_a_form_the_client_cannot_read_are_a_failure_not_an_empty_list() {
        let server = FakeHttp::default().reply(
            Method::Get,
            "/api/classroom/courses",
            200,
            r#"{"courses":[{"id":"","name":"Math","students":[]}]}"#,
        );
        let ApiError::Failed(cause) = block_on(api(&server).courses()).unwrap_err() else {
            panic!("a broken payload is a failure");
        };
        assert!(cause.contains("cannot read"), "{cause}");
    }

    #[test]
    fn signing_out_posts_and_an_ended_session_reads_as_unauthorized() {
        let server = FakeHttp::default().reply(Method::Post, "/auth/logout", 204, "");
        assert_eq!(block_on(api(&server).logout()), Ok(()));
        assert_eq!(
            server.requests(),
            [(Method::Post, "/auth/logout".to_owned(), None)]
        );

        let server = FakeHttp::default().reply(Method::Post, "/auth/logout", 401, "");
        assert_eq!(block_on(api(&server).logout()), Err(ApiError::Unauthorized));
    }

    #[test]
    fn methods_and_errors_read_as_text() {
        assert_eq!(Method::Get.as_str(), "GET");
        assert_eq!(Method::Put.as_str(), "PUT");
        assert_eq!(Method::Post.as_str(), "POST");
        assert_eq!(
            ApiError::Failed("the server answered 500".to_owned()).to_string(),
            "the server answered 500"
        );
        assert_eq!(ApiError::Unauthorized.to_string(), "you are not signed in");
        assert!(ApiError::Reauthorize.to_string().contains("sign in again"));
    }

    #[test]
    fn only_short_lowercase_codes_are_shown() {
        assert_eq!(short_code("access_denied"), Some("access_denied"));
        assert_eq!(short_code("state2"), Some("state2"));
        for refused in [
            "",
            "Access Denied",
            "call 555-0100 now",
            "<b>x</b>",
            "a-b",
            &"x".repeat(41),
        ] {
            assert_eq!(short_code(refused), None, "{refused}");
        }
    }

    #[test]
    fn a_hostile_error_code_is_not_echoed() {
        let server = FakeHttp::default().reply(
            Method::Put,
            "/api/doc",
            500,
            r#"{"error":"Call 555-0100 to fix this"}"#,
        );
        assert_eq!(
            block_on(api(&server).put_doc("{}")),
            Err(ApiError::Failed("the server answered 500".to_owned()))
        );
    }
}
