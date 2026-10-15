//! Group name generator. Alliteration is structural: the lexicon is keyed by letter and a name is
//! only ever built from one bucket, so an unalliterative name has no code path.

use super::label::Label;
use super::rng::{Rng, choose};

struct Bucket {
    letter: char,
    adjectives: &'static [&'static str],
    animals: &'static [&'static str],
}

/// Upper bound on groups, so a unique name always exists. The capacity assert below enforces it.
pub const MAX_GROUPS: usize = 40;

/// Kid-friendly words only. Animals are plural, ready to print. Buckets stay in letter order.
const LEXICON: &[Bucket] = &[
    Bucket {
        letter: 'A',
        adjectives: &["Adorable", "Amazing", "Agile", "Adventurous"],
        animals: &["Alpacas", "Antelopes", "Armadillos", "Albatrosses"],
    },
    Bucket {
        letter: 'B',
        adjectives: &["Brave", "Bouncy", "Bright", "Bold"],
        animals: &["Badgers", "Bears", "Bunnies", "Beavers"],
    },
    Bucket {
        letter: 'C',
        adjectives: &["Clever", "Cheerful", "Cosmic", "Curious"],
        animals: &["Cheetahs", "Capybaras", "Crabs", "Chipmunks"],
    },
    Bucket {
        letter: 'D',
        adjectives: &["Dazzling", "Daring", "Dapper", "Dreamy"],
        animals: &["Dolphins", "Ducks", "Donkeys", "Dragonflies"],
    },
    Bucket {
        letter: 'E',
        adjectives: &["Eager", "Electric", "Epic", "Excellent"],
        animals: &["Eagles", "Elephants", "Emus", "Elks"],
    },
    Bucket {
        letter: 'F',
        adjectives: &["Fantastic", "Fearless", "Friendly", "Fuzzy"],
        animals: &["Flamingos", "Foxes", "Frogs", "Ferrets"],
    },
    Bucket {
        letter: 'G',
        adjectives: &["Gentle", "Giggly", "Glowing", "Graceful"],
        animals: &["Giraffes", "Geckos", "Gophers", "Gazelles"],
    },
    Bucket {
        letter: 'H',
        adjectives: &["Happy", "Heroic", "Hopeful", "Hearty"],
        animals: &["Hedgehogs", "Hamsters", "Herons", "Hummingbirds"],
    },
    Bucket {
        letter: 'I',
        adjectives: &["Incredible", "Inventive", "Iridescent", "Imaginative"],
        animals: &["Iguanas", "Ibises", "Impalas", "Ibexes"],
    },
    Bucket {
        letter: 'J',
        adjectives: &["Jolly", "Jazzy", "Joyful", "Jumpy"],
        animals: &["Jaguars", "Jackrabbits", "Jellyfish", "Jays"],
    },
    Bucket {
        letter: 'K',
        adjectives: &["Keen", "Kind", "Kooky", "Kindhearted"],
        animals: &["Kangaroos", "Koalas", "Kittens", "Kingfishers"],
    },
    Bucket {
        letter: 'L',
        adjectives: &["Lively", "Lucky", "Lovely", "Loyal"],
        animals: &["Lemurs", "Llamas", "Lions", "Ladybugs"],
    },
    Bucket {
        letter: 'M',
        adjectives: &["Majestic", "Mighty", "Merry", "Marvelous"],
        animals: &["Monkeys", "Moose", "Meerkats", "Manatees"],
    },
    Bucket {
        letter: 'N',
        adjectives: &["Nifty", "Nimble", "Noble", "Neat"],
        animals: &["Narwhals", "Newts", "Nightingales", "Numbats"],
    },
    Bucket {
        letter: 'O',
        adjectives: &["Optimistic", "Outstanding", "Overjoyed", "Orange"],
        animals: &["Otters", "Owls", "Orcas", "Ostriches"],
    },
    Bucket {
        letter: 'P',
        adjectives: &["Peppy", "Plucky", "Polite", "Playful"],
        animals: &["Penguins", "Pandas", "Puffins", "Ponies"],
    },
    Bucket {
        letter: 'R',
        adjectives: &["Radiant", "Remarkable", "Rapid", "Rosy"],
        animals: &["Rabbits", "Raccoons", "Reindeer", "Robins"],
    },
    Bucket {
        letter: 'S',
        adjectives: &["Sunny", "Speedy", "Sparkly", "Silly"],
        animals: &["Sloths", "Seals", "Squirrels", "Sparrows"],
    },
    Bucket {
        letter: 'T',
        adjectives: &["Terrific", "Tiny", "Thoughtful", "Tremendous"],
        animals: &["Tigers", "Turtles", "Toucans", "Tortoises"],
    },
    Bucket {
        letter: 'W',
        adjectives: &["Whimsical", "Wild", "Wonderful", "Wise"],
        animals: &["Whales", "Wombats", "Walruses", "Woodpeckers"],
    },
];

const fn words_start_with(words: &[&str], letter: char) -> bool {
    if words.is_empty() {
        return false;
    }
    let mut i = 0;
    while i < words.len() {
        let bytes = words[i].as_bytes();
        if bytes.is_empty() || bytes[0] as char != letter {
            return false;
        }
        i += 1;
    }
    true
}

const fn lexicon_is_alliterative(lexicon: &[Bucket]) -> bool {
    let mut i = 0;
    while i < lexicon.len() {
        let bucket = &lexicon[i];
        if !words_start_with(bucket.adjectives, bucket.letter)
            || !words_start_with(bucket.animals, bucket.letter)
        {
            return false;
        }
        if i > 0 && lexicon[i - 1].letter >= bucket.letter {
            return false;
        }
        i += 1;
    }
    true
}

const fn lexicon_capacity(lexicon: &[Bucket]) -> usize {
    let mut total = 0;
    let mut i = 0;
    while i < lexicon.len() {
        total += lexicon[i].adjectives.len() * lexicon[i].animals.len();
        i += 1;
    }
    total
}

const _: () = assert!(
    lexicon_is_alliterative(LEXICON),
    "a lexicon word does not start with its bucket letter, or buckets are out of order"
);
const _: () = assert!(
    lexicon_capacity(LEXICON) > MAX_GROUPS,
    "lexicon is too small for MAX_GROUPS unique names"
);

/// A bucket that still has at least one pair no used name occupies.
struct Candidate {
    letter: char,
    free: Vec<(&'static str, &'static str)>,
}

/// Draws one name that is not in `used`.
///
/// Prefers a bucket whose letter no used name starts with, then any bucket with a free pair.
/// A free pair always exists because `used` holds at most `MAX_GROUPS` names and the lexicon
/// holds more, so exhaustion is a broken invariant and panics.
pub fn draw_name(used: &[&Label], rng: &mut impl Rng) -> Label {
    let is_used = |adjective: &str, animal: &str| {
        used.iter().any(|name| {
            name.as_str()
                .strip_prefix(adjective)
                .and_then(|rest| rest.strip_prefix(' '))
                == Some(animal)
        })
    };

    let candidates: Vec<Candidate> = LEXICON
        .iter()
        .map(|bucket| Candidate {
            letter: bucket.letter,
            free: bucket
                .adjectives
                .iter()
                .flat_map(|&adjective| {
                    bucket
                        .animals
                        .iter()
                        .map(move |&animal| (adjective, animal))
                })
                .filter(|&(adjective, animal)| !is_used(adjective, animal))
                .collect(),
        })
        .filter(|candidate| !candidate.free.is_empty())
        .collect();

    let fresh_letter: Vec<&Candidate> = candidates
        .iter()
        .filter(|candidate| {
            !used
                .iter()
                .any(|name| name.as_str().starts_with(candidate.letter))
        })
        .collect();
    let pool: Vec<&Candidate> = if fresh_letter.is_empty() {
        candidates.iter().collect()
    } else {
        fresh_letter
    };

    let candidate = choose(&pool, rng).expect("lexicon capacity exceeds MAX_GROUPS");
    let &(adjective, animal) = choose(&candidate.free, rng).expect("candidates have a free pair");
    Label::parse(&format!("{adjective} {animal}")).expect("lexicon names fit in a Label")
}

/// `count` distinct names, drawn one at a time so each sees the earlier ones as used. Once
/// `count` exceeds the number of letters the letter preference falls back to any free pair.
pub fn draw_names(count: usize, rng: &mut impl Rng) -> Vec<Label> {
    let mut names: Vec<Label> = Vec::with_capacity(count);
    for _ in 0..count {
        let used: Vec<&Label> = names.iter().collect();
        let next = draw_name(&used, rng);
        names.push(next);
    }
    names
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::domain::rng::SplitMix64;

    fn first_letter(name: &Label) -> char {
        name.as_str().chars().next().unwrap()
    }

    fn words(name: &Label) -> (&str, &str) {
        name.as_str().split_once(' ').unwrap()
    }

    fn in_lexicon(name: &Label) -> bool {
        let (adjective, animal) = words(name);
        LEXICON
            .iter()
            .any(|b| b.adjectives.contains(&adjective) && b.animals.contains(&animal))
    }

    #[test]
    fn lexicon_has_enough_letters_and_words() {
        assert!(LEXICON.len() >= 14);
        assert!(lexicon_capacity(LEXICON) > MAX_GROUPS);
    }

    #[test]
    fn every_lexicon_pair_makes_a_valid_alliterative_label() {
        for bucket in LEXICON {
            for adjective in bucket.adjectives {
                for animal in bucket.animals {
                    let name = Label::parse(&format!("{adjective} {animal}")).unwrap();
                    assert_eq!(first_letter(&name), bucket.letter);
                }
            }
        }
    }

    #[test]
    fn names_are_alliterative_and_come_from_the_lexicon() {
        for seed in 0..40 {
            for name in draw_names(MAX_GROUPS, &mut SplitMix64::seeded(seed)) {
                let (adjective, animal) = words(&name);
                assert_eq!(adjective.chars().next(), animal.chars().next());
                assert!(in_lexicon(&name), "{name} is not a lexicon pair");
            }
        }
    }

    #[test]
    fn every_group_count_up_to_the_cap_gets_unique_names() {
        for seed in 0..10 {
            for count in 1..=MAX_GROUPS {
                let names = draw_names(count, &mut SplitMix64::seeded(seed));
                assert_eq!(names.len(), count);
                let unique: HashSet<_> = names.iter().collect();
                assert_eq!(unique.len(), count, "seed {seed} count {count}");
            }
        }
    }

    #[test]
    fn small_groupings_use_a_different_letter_per_group() {
        for seed in 0..40 {
            let names = draw_names(LEXICON.len(), &mut SplitMix64::seeded(seed));
            let letters: HashSet<_> = names.iter().map(first_letter).collect();
            assert_eq!(letters.len(), LEXICON.len());
        }
    }

    #[test]
    fn draw_name_avoids_names_already_used() {
        let mut rng = SplitMix64::seeded(5);
        let taken = draw_names(MAX_GROUPS, &mut SplitMix64::seeded(1));
        let used: Vec<&Label> = taken.iter().collect();
        for _ in 0..200 {
            let fresh = draw_name(&used, &mut rng);
            assert!(!taken.contains(&fresh));
        }
    }

    #[test]
    fn draw_name_finds_the_last_free_pair() {
        let every_pair: Vec<Label> = LEXICON
            .iter()
            .flat_map(|b| {
                b.adjectives.iter().flat_map(move |a| {
                    b.animals
                        .iter()
                        .map(move |n| Label::parse(&format!("{a} {n}")).unwrap())
                })
            })
            .collect();
        let spare = every_pair[7].clone();
        let used: Vec<&Label> = every_pair.iter().filter(|name| **name != spare).collect();
        assert_eq!(draw_name(&used, &mut SplitMix64::seeded(0)), spare);
    }

    #[test]
    fn hand_typed_names_do_not_confuse_the_generator() {
        let teacher_named = [
            Label::parse("Team Rocket").unwrap(),
            Label::parse("Brave").unwrap(),
            Label::parse("Brave Bears").unwrap(),
        ];
        let used: Vec<&Label> = teacher_named.iter().collect();
        for seed in 0..50 {
            let name = draw_name(&used, &mut SplitMix64::seeded(seed));
            assert!(in_lexicon(&name));
            assert!(!teacher_named.contains(&name));
        }
    }
}
