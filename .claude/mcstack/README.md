# mcstack, vendored

`.claude/skills/*` (every skill except `verify-tivan` and the Cloudflare ones),
`.claude/agents/bde-agent.md`, and `.claude/agents/comment-sicko.md` are a copy of
the [mcstack](https://github.com/volker48/mcstack) plugin, so the skills load in
cloud sessions. Cloud sessions don't load plugins from a repo's
`.claude/settings.json`, but they do load project skills and agents.

- Source: `plugins/mcstack` at volker48/mcstack commit `99805bb` (plugin version 0.0.3),
  plus the `bde-mode/scripts/worktree-audit.sh` fix from
  [volker48/mcstack#2](https://github.com/volker48/mcstack/pull/2) (GNU/Linux `stat` and `date` flags)
- Not copied: `setup-mcstack` (it only writes `~/.claude/mcstack-models.md`; without
  that file every role uses its default model), `docs/`, and the plugin README.
- License and attribution: `LICENSE` and `THIRD_PARTY_NOTICES` in this folder.

## Changes from the plugin

A project skill is not a plugin, so three things were rewritten:

1. `${CLAUDE_PLUGIN_ROOT}/skills/...` became `.claude/skills/...`
   (`bde-mode/SKILL.md`, `agents/bde-agent.md`). The `~/.claude/plugins` search
   fallback in `bde-agent` was removed.
2. `subagent_type: "mcstack:bde-agent"` and `"mcstack:comment-sicko"` became
   `"bde-agent"` and `"comment-sicko"`. Project agents have no plugin prefix.
3. `plugins/mcstack/skills/...` paths in `bde-mode/playbooks/multi-phase-plan.md`
   became `.claude/skills/...`, and relative links from `bde-help` to the plugin's
   README, guide pages, and `setup-mcstack` became GitHub URLs.

## Updating

This is a one-time copy. To pick up a newer mcstack, copy the skills and agents
over again and repeat the three rewrites above.
