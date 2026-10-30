use corten::prelude::*;

/// The save note's id: every save answers with the note, which replaces it.
#[derive(Id)]
pub struct SaveNoteId;

/// What the server said to the last save, with the deck's version after it (and, for a saved one,
/// which of the page's sends was saved). The page's script reads the outcome; the reader reads the
/// note's text.
pub enum Answer {
    /// Nothing yet: the page as loaded.
    None,
    Saved {
        version: u64,
        draft: u64,
    },
    Refused {
        version: u64,
        message: String,
    },
    /// `unsure`: a save the server no longer remembers may have been this one (`Outcome::Stale`).
    Stale {
        version: u64,
        unsure: bool,
    },
}

impl Answer {
    /// What the script compares: `saved`, `refused`, `stale`, or nothing.
    fn outcome(&self) -> &'static str {
        match self {
            Answer::None => "",
            Answer::Saved { .. } => "saved",
            Answer::Refused { .. } => "refused",
            Answer::Stale { .. } => "stale",
        }
    }

    fn version(&self) -> u64 {
        match self {
            Answer::None => 0,
            Answer::Saved { version, .. } | Answer::Refused { version, .. } | Answer::Stale { version, .. } => *version,
        }
    }

    fn draft(&self) -> u64 {
        match self {
            Answer::Saved { draft, .. } => *draft,
            _ => 0,
        }
    }

    fn unsure(&self) -> bool {
        matches!(self, Answer::Stale { unsure: true, .. })
    }
}

/// A save's answer, told to the builder around the note: what became of it (`saved`, `refused`,
/// `stale`), the deck's version after it, which send was saved, and, when stale, whether a save
/// the server no longer remembers may have been it. It bubbles from the note.
#[corten::script::event]
pub struct DeckAnswered {
    pub outcome: String,
    pub version: u64,
    pub draft: u64,
    pub unsure: bool,
}

/// What became of the last save, in words, inside the builder's status line. Each save answers
/// with this note as a patch; as it joins the page its script tells the builder the outcome (a
/// `DeckAnswered` event), which then takes the version, rolls a refused change back, or looks
/// for another tab's save. Without JavaScript it is part of the page a refused save answers with.
#[component(js)]
pub fn SaveNote<'a>(answer: &'a Answer) -> impl Patch {
    html! {
        <span
            id={SaveNoteId}
            on:connect={js::answered(target, answer.outcome(), answer.version(), answer.draft(), answer.unsure())}
        >
            match answer {
                Answer::None => {}
                Answer::Saved { .. } => { "Saved." }
                Answer::Refused { message, .. } => { "Not saved: "{message} }
                Answer::Stale { .. } => { "Not saved: this deck was changed in another tab." }
            }
        </span>
    }
}
