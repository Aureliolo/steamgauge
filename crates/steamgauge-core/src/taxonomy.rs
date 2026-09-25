//! The fixed sheet of categories.
//!
//! These are the categories that stay the same across every game, so numbers from different
//! corpora can be compared. Game-specific categories are induced separately and layered on
//! top; nothing here is meant to describe any particular game well.
//!
//! Each category carries a *description written the way a review would say it*, not an
//! abstract label. The classifier compares review vectors against these descriptions, and
//! embedding models place "the frame rate tanks in cities" much nearer to a sentence about
//! stuttering than to the word "performance".
//!
//! A category also carries a **boundary rule**, which is written for whoever is labelling
//! and is deliberately never embedded. Rules are about how to choose between two categories
//! and read like instructions; feeding "prefer difficulty when the complaint is about
//! balance" to an embedding model would drag the anchor towards the vocabulary of
//! instructions and away from the vocabulary of reviews.

/// One category in the taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Category {
    /// Stable identifier. Appears in output files, so changing one invalidates comparisons.
    pub id: &'static str,
    pub label: &'static str,
    /// Embedded verbatim. Must read like the reviews it is meant to attract.
    pub description: &'static str,
    /// How to choose when this category competes with another. Never embedded.
    pub boundary: Option<&'static str>,
    /// Whether a review in this category can be in no other.
    ///
    /// A claim about absence cannot share a review with a claim about presence. "Says
    /// nothing about the game" and "a verdict with no reason given" are both statements that
    /// no aspect was named, so pairing either with an aspect is a contradiction rather than a
    /// second subject, and the labellers read them that way without being told to: across
    /// the six reference sets `offtopic` is alone in 191 labels of 191 and `verdict` in 364
    /// of 374.
    ///
    /// Measured on 2026-09-09, this trades one kind of error for another and leaves the
    /// pooled macro F1 where it found it, at 0.585 against 0.584. What it buys is precision:
    /// `offtopic` goes from 0.47 to 0.56 and `verdict` from 0.74 to 0.84, because neither can
    /// be hedged onto a review whose subject the classifier has already named. What it costs
    /// is the credit those hedges used to earn: `verdict` recall falls from 0.70 to 0.57,
    /// which is the honest figure for how often a bare verdict is actually identified as one.
    /// Kept for the reason it was added rather than for the score: a classifier that can
    /// report a review as naming an aspect and naming none is wrong whatever it scores.
    pub alone: bool,
}

/// What the categories are, as a hash of their ids in order.
///
/// A model's output means whatever the categories mean, so a model trained when this was
/// different is answering a different question and every number it produces is mislabelled.
/// This is what a stored reading is checked against.
///
/// Deliberately not the wording: a boundary rule that moves changes what a *labeller* should
/// answer and changes nothing about what a model already emitted, so charging a re-read for a
/// clarified sentence would be a lie about what went stale.
/// Six bytes of a digest as hex. Short enough to read aloud in a bug report and still far
/// past any chance of two sheets colliding.
fn short_hex(digest: &[u8]) -> String {
    use std::fmt::Write as _;

    digest.iter().take(6).fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

/// The categories this build has, as their ids.
///
/// Written out in full wherever a reading or a model records what it answered, rather than
/// reduced to a name or a hash. A reader of the file can see what the thing meant, and a
/// mismatch can say which categories differ instead of reporting that two opaque strings are
/// not equal.
#[must_use]
pub fn categories() -> Vec<String> {
    SHEET
        .iter()
        .map(|category| category.id.to_owned())
        .collect()
}

/// Whether something recorded under `was` is answering the categories this build has.
///
/// Order is not part of it: a model numbers its classes however its training data did, and
/// the sheet is read as a set of ids.
#[must_use]
pub fn categories_still_mean(was: &[String]) -> bool {
    let mut had: Vec<&str> = was.iter().map(String::as_str).collect();
    let mut has: Vec<&str> = SHEET.iter().map(|category| category.id).collect();
    had.sort_unstable();
    has.sort_unstable();
    had == has
}

/// The categories one side has and the other does not, both ways round, for an error that
/// says what actually differs.
#[must_use]
pub fn categories_differ(was: &[String]) -> String {
    let has: Vec<&str> = SHEET.iter().map(|category| category.id).collect();
    let gone: Vec<&str> = was
        .iter()
        .map(String::as_str)
        .filter(|id| !has.contains(id))
        .collect();
    let added: Vec<&str> = has
        .iter()
        .copied()
        .filter(|id| !was.iter().any(|was| was == id))
        .collect();
    match (gone.is_empty(), added.is_empty()) {
        (true, true) => "the same categories in another order".to_owned(),
        (false, true) => format!("without {}", gone.join(", ")),
        (true, false) => format!("missing {}", added.join(", ")),
        (false, false) => format!("without {}, missing {}", gone.join(", "), added.join(", ")),
    }
}

/// What the sheet says, as a hash of the whole brief a labeller is handed.
///
/// A label answers the sheet its labeller read, boundary rules included, so this moves whenever
/// any of that wording does. It is recorded on every label, and it is how `revisit` knows which
/// labels predate a clarification. Computed rather than named, because the one time a version
/// was assigned by hand it was forgotten, and two incompatible sheets both called themselves
/// the same thing.
#[must_use]
pub fn sheet() -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(labelling_brief(Unit::Claim).as_bytes());
    short_hex(&hasher.finalize())
}

pub const SHEET: &[Category] = &[
    Category {
        id: "performance",
        label: "Performance",
        description: "The game runs badly. Low frame rate, stuttering, frame drops, poor \
                      optimisation, long loading times, and it struggles even on good hardware.",
        boundary: Some(
            "Frame rate and stuttering belong here. How the animation itself looks, and how \
             fast it plays out, belong to graphics.",
        ),
        alone: false,
    },
    Category {
        id: "bugs",
        label: "Bugs and crashes",
        description: "The game is broken and buggy. It crashes to desktop, freezes, corrupts \
                      or loses save files, and is full of glitches that block progress.",
        boundary: Some(
            "Something that worked and now does not belongs here, a headset included, and so \
             does an option that is there and does nothing. Being unable to log in is \
             here when the login itself is broken and policy when needing an account at all \
             is the complaint. A game that never starts, with no reason given, is \
             compatibility.",
        ),
        alone: false,
    },
    // Narrow on purpose. Phrased as "how the game plays and whether it is fun" this became
    // the nearest match for any general discussion and took 47.7% of primaries, which is a
    // property of the wording rather than a finding about any game.
    Category {
        id: "gameplay",
        label: "Gameplay and mechanics",
        description: "The systems and mechanics themselves. Combat, movement, crafting, \
                      building, exploration, progression systems, and whether the mechanics \
                      are deep or shallow.",
        boundary: Some(
            "Naming the genre or comparing the game to another one belongs to genre, unless the \
             claim is only a judgement with the genre attached, which is verdict. Whether \
             options are balanced against each other belongs to difficulty, and so does how \
             hard an enemy is to beat; how that enemy behaves and what it does is a mechanic \
             and belongs here. How much game there is belongs to content, and anything \
             user-made belongs to mods. A system the game never explains belongs to tutorial, \
             however good the system itself is.",
        ),
        alone: false,
    },
    // 35% of reviews in the first corpus measured named a genre or compared the game to
    // another one. With nowhere to put that, all of it landed in gameplay, which is most of
    // why gameplay held half of all primaries.
    Category {
        id: "genre",
        label: "Genre and comparisons",
        description: "What kind of game this is. A roguelike, turn-based tactics, a \
                      management sim, a deckbuilder, a soulslike. It plays like FTL, it is \
                      XCOM meets Darkest Dungeon, it reminds me of the old games in the \
                      genre.",
        boundary: Some(
            "Naming what kind of game it is, or which games it resembles, belongs here, and \
             that includes calling it a co-op game or comparing it to other co-op games. As \
             soon as a review says a mechanic is deep, shallow, satisfying or broken, that \
             part is gameplay, and whether the co-op itself works is multiplayer. Ask what the \
             claim is for: if it is praising or condemning, and the genre or the other game is \
             only the noun being judged, it is verdict however specific that noun is, so \
             \"great platformer\", \"god tier city builder\", \"the best roguelike out there\" \
             and \"better than Slay the Spire\" are all verdict. It is genre only when the kind \
             of game, or the resemblance, is the information being conveyed: \"XCOM meets \
             Darkest Dungeon\", \"a deckbuilder with no deck\", \"not really a soulslike at \
             all\", \"I normally hate management sims\". That holds for any comparison and not \
             only with this game's own predecessor: better or worse is a verdict, how it \
             differs is genre. A recommendation is a verdict whether or not it names a \
             condition or another game: \"if you like horror and survival this is perfect for \
             you\", \"for fans of MetroidVanias this might be worth trying\" and \"if you \
             liked Slay the Spire you will like this\" all say who should buy it, which is a \
             judgement. A bare list of what the game is about, \"dwarves, beer, space, \
             guns, bugs\", with no judgement attached, is genre too: it says what kind of game \
             this is and nothing else.",
        ),
        alone: false,
    },
    // Reviews that give a verdict and name no aspect are common, and without a home they
    // contaminate whichever category happens to sit nearest in the embedding space.
    Category {
        id: "verdict",
        label: "Overall verdict only",
        description: "A verdict with no specific reason given. Great game, terrible game, \
                      ten out of ten, would recommend, do not buy, best game ever, an \
                      excellent city builder, I find this game childish, please make a \
                      sequel.",
        boundary: Some(
            "Only when no aspect is named at all. A review that gives a verdict and then \
             names one specific thing belongs to that thing: \"super fun, and the story is \
             great\" is story. A judgement with only the kind of game attached, \"excellent \
             city builder\", is a verdict; genre is for when what kind of game it is, or \
             which game it resembles, is the point being made. A recommendation is a verdict \
             whatever condition it carries: \"if you like horror this is perfect for you\" \
             recommends the game to a group of people, and who should buy it is a judgement. \
             It stays a verdict when the condition names what the reader likes, \"if you like \
             good graphics and a real story\", because that says who the game is for. It \
             leaves when the condition is a fact about the reader's hardware or body rather \
             than their taste, and belongs to that fact's row: \"skip it if you own a Quest\" \
             is VR and \"not for anyone who struggles with motor skills\" is accessibility, \
             because the reader is told whether the game will work for them, not whether they \
             will like it. Saying \
             it plays like another game, \"if you like Marvel vs Capcom 2, this is very \
             similar\", conveys the resemblance and is genre. \
             A community's own catchphrase used as a salute, \"Rock and Stone\", is a verdict \
             and it is praise. Asking for a sequel belongs here; asking for a port belongs to \
             compatibility. Looking forward to the game, or wishing the reader a good time \
             with it, judges nothing and is offtopic.",
        ),
        alone: true,
    },
    // Joke and meme reviews were being counted as verdicts, which inflates the one category
    // whose whole purpose is to be the honest home of reviews that say nothing specific.
    Category {
        id: "offtopic",
        label: "Says nothing about the game",
        description: "The review is not about the game. A joke, a meme, a copypasta, a story \
                      about the reviewer's day, an argument with another reviewer, a \
                      complaint about Steam or the shop page, or a protest about something \
                      the publisher did elsewhere.",
        boundary: Some(
            "A joke that still makes a point about the game belongs to whatever it is joking \
             about, most often gameplay or difficulty, and a protest about anything this \
             game's publisher or platform did belongs to policy. This is only for reviews from \
             which a reader would learn nothing at all, and that includes a review with \
             nothing in it: a full stop, a row of emoji, a keyboard mash. A single word that \
             does pass judgement, good or terrible or 10/10, is a verdict, and so is one that \
             carries an attitude without naming anything, \"yum\", \"god\", \"finally\". A word \
             carrying no attitude at all, a bare title like \"WARHAMMER\" or a noise like \
             \"ooookkkk\", is offtopic. Looking forward to playing, \"I am so excited to play \
             this\", and telling the reader to enjoy it, \"have fun\", are about the reviewer \
             and the reader rather than about the game, and belong here: a verdict needs a \
             judgement of the game, however bare. Where you cannot tell whether a fragment \
             carries an attitude, that is what the ambiguous flag is for; do not force it \
             either way.",
        ),
        alone: true,
    },
    Category {
        id: "story",
        label: "Story and writing",
        description: "The story, plot and characters. The writing, the dialogue, the humour, \
                      the ending, the world and its lore, the characters players grew fond \
                      of, and whether the narrative is worth following.",
        boundary: Some(
            "Anecdotes from a playthrough belong here when they are about a character or an \
             event, and to gameplay when they are about a mechanic. Humour in the writing is \
             here; a charming art style is graphics. How much story there is belongs to \
             content, the same way how much game there is does: \"the main story is fairly \
             short\" is content, and this is what the narrative is and whether it is worth \
             following.",
        ),
        alone: false,
    },
    // Asked for by nine labellers across two eras and two genres, which is more independent
    // evidence than any other category in this sheet had. Every labeller of Alien: Isolation
    // named it, the DEVOUR labeller reached it from a different corpus, and all three claim
    // labellers on a city builder called it the largest gap in the sheet. Without it, "you
    // never FEEL it" and "the deaths are just numbers now" fall to story or gameplay at low
    // confidence, and a report about a horror game cannot say that players found it
    // frightening.
    Category {
        id: "atmosphere",
        label: "Atmosphere and feel",
        description: "What the game makes you feel while you are playing it. Genuinely \
                      terrifying, the tension never lets up, put headphones on and turn the \
                      lights off, it pulls you in and you lose whole evenings, the mood is \
                      oppressive and lonely, it has lost the soul the first one had, the \
                      people are just numbers to you now, you never really feel it.",
        boundary: Some(
            "This is the feeling itself, for reviews that name nothing that produces it. \
             Being unable to stop playing, losing whole evenings and calling it addictive are \
             here, and so is a reaction with nothing named behind it: it made me cry, the \
             feels, I was terrified. Where a review says what creates the mood, that part \
             belongs to what creates it: a frightening creature design is graphics, a \
             soundtrack that unsettles is audio, a mechanic that keeps you on edge is \
             gameplay, faithfulness to the film or book it adapts is licensing, and so is \
             feeling as if you are inside that film, and immersion broken by a crash is bugs.",
        ),
        alone: false,
    },
    Category {
        id: "graphics",
        label: "Graphics and art",
        description: "How the game looks. The visuals, the art style, the animation and how \
                      quickly it plays out, the environments and character models, whether \
                      it is beautiful or ugly or charming.",
        boundary: Some(
            "Animation quality and animation speed belong here. Frame rate belongs to \
             performance. Motion sickness in a game played on a screen, from the camera, head \
             bob or field of view, belongs here, and so does flashing that can bring on a \
             seizure; the option to turn either down is accessibility, and sickness in a \
             headset is VR. Sickness with neither a screen nor a headset named in the claim is \
             settled by the review around it: one about playing in a headset makes it VR, and \
             one that never mentions a headset is about a screen, and here.",
        ),
        alone: false,
    },
    Category {
        id: "audio",
        label: "Audio and music",
        description: "How the game sounds. The soundtrack, the music, the sound effects, the \
                      voice acting and the audio mixing.",
        boundary: None,
        alone: false,
    },
    Category {
        id: "controls",
        label: "Controls and interface",
        description: "The controls and the user interface. Clunky or responsive controls, \
                      controller support, menus, the HUD, the inventory screens, and how many \
                      clicks it takes to do anything.",
        boundary: Some(
            "Whether a gamepad is supported belongs here, and so does anything the game makes \
             you sit through or click past: a cutscene that cannot be skipped is a question of \
             how many clicks it takes to do anything. Whether a headset's tracked controllers \
             are detected and supported belongs to VR, with the headset they come with; what \
             the game binds to them, how its scheme feels in the hand and where it puts your \
             hands and body are here, in a headset or out of one. Whether the game runs on a \
             given device belongs to compatibility. How the keys are laid out and whether \
             they respond is here. Whether the controls can be remapped is accessibility, \
             whatever the reviewer wants it for, \"I wish I could remap the keys\" and \"every \
             button can be rebound to taste\" alike: some players cannot play on the default \
             layout at all, and the option exists for them. That includes a remapping that \
             exists and falls short, keys it will not free or a control it will not unbind; one \
             that does nothing when it is used is bugs. A complaint about the default layout \
             that names remapping only as its cure, \"you will have to remap everything\", is \
             about the layout, and here. Every other setting turns on the \
             same need: one somebody needs in order to play at all, subtitles, a toggle for a \
             button you would otherwise hold, is accessibility; one that is a matter of taste, \
             aim assist, aim acceleration, a deadzone, the sensitivity, is here; and an option \
             that is there and does not work, a remap that does not remap, is bugs. The \
             settings menu itself, how many options it holds and display modes such as \
             fullscreen and windowed, belongs here, and so does praise for its breadth that \
             lists accessibility among the rest, \"plenty of options, from accessibility to \
             graphics\"; a claim about the accommodations themselves is accessibility. Which \
             graphics settings give a playable frame rate is performance.",
        ),
        alone: false,
    },
    Category {
        id: "difficulty",
        label: "Difficulty and balance",
        description: "How hard the game is and whether it is fair. Difficulty spikes, \
                      grinding, whether the options are balanced against each other, luck and \
                      randomness deciding the outcome, and whether it is too easy or punishing.",
        boundary: Some(
            "Balance complaints belong here rather than to gameplay, including when they name \
             a specific mechanic as overpowered or useless. How hard an enemy is to beat is \
             here; how it is designed and what it does is gameplay. A complaint about what a \
             patch changed is about the change and belongs here where the change was to \
             balance, with updates for the patching itself. Being lost because nothing was \
             explained belongs to tutorial. What a purchase gives you is monetisation, and \
             how it plays once you have it is here.",
        ),
        alone: false,
    },
    Category {
        id: "content",
        label: "Amount of content",
        description: "How much game there is. Length, how many hours it lasts, replay value, \
                      whether runs differ from one another, whether it gets repetitive, and \
                      whether it feels finished or thin and runs out of things to do.",
        boundary: Some(
            "Replayability and repetitiveness are two ends of one axis and both belong here, \
             never to gameplay. What the players made rather than the developers belongs to \
             mods, however much of the game it accounts for.",
        ),
        alone: false,
    },
    // The most-reported gap in this taxonomy's history: five games at review level, three more
    // at claim level, and one labeller calling modding a game's dominant theme while filing it
    // under `content` at low confidence with every claim marked contested. The model said the
    // same thing independently: the game it declined most of, 80% against 61% on the least, was
    // that one. Across the labelled sets a claim naming mods lands in ten different rows.
    Category {
        id: "mods",
        label: "Mods and user content",
        description: "Mods and what players have made. Whether the game supports modding, how \
                      easy mods are to install, the workshop, what the community has built, \
                      whether the game is worth playing unmodded, and mods breaking when the \
                      game updates.",
        boundary: Some(
            "Anything made by players rather than by the developers belongs here, including \
             how much of the game's life it accounts for. Whether the developers support \
             modding is here; whether a patch broke the mods is here too, because the subject \
             is the mods. Paid mods and creator programmes are monetisation, and what the \
             people who make them are like is community.",
        ),
        alone: false,
    },
    Category {
        id: "price",
        label: "Price and value",
        description: "What it costs and whether it is worth the money. Worth every penny, a \
                      waste of money, I refunded it, full price versus sale, value for money, \
                      and whether to wait for a discount.",
        boundary: Some(
            "What the base game costs belongs here, and so does judging its value in money: \
             worth every penny and waste of money are both about what it was worth paying. \
             What is sold on top of it belongs to monetisation.",
        ),
        alone: false,
    },
    Category {
        id: "monetisation",
        label: "Monetisation and DLC",
        description: "Paid extras and how they are sold. Microtransactions, battle passes, \
                      loot boxes, paywalls, season passes and content cut out to sell as DLC.",
        boundary: Some(
            "What is sold on top of the game, and how, belongs here, including what a purchase \
             gives you: a pack whose contents are decided by chance is sold here, and whether \
             luck then decides the match is difficulty. Wanting more of the game belongs to \
             content even when the review asks for it as DLC: that is a review saying it ran \
             out, not one about how the game is sold.",
        ),
        alone: false,
    },
    Category {
        id: "multiplayer",
        label: "Multiplayer and online",
        description: "Playing with or against other people. Matchmaking, servers, lag and ping, \
                      co-op, player counts, whether lobbies are dead and whether cheaters ruin it.",
        boundary: Some(
            "Servers, matchmaking and connection quality belong here, and so does what playing \
             with other people is like: better with friends, dull on your own, worth it only \
             in a group. What the other players are like belongs to community, and calling \
             the game co-op as a kind of game belongs to genre. \"The game is dead\" is here \
             when nobody is playing it and updates when nobody is developing it.",
        ),
        alone: false,
    },
    Category {
        id: "community",
        label: "Community and players",
        description: "The people who play it. Whether the community is welcoming or toxic, \
                      griefing and harassment, and what the playerbase is like.",
        boundary: Some(
            "What the people are like belongs here. What they have made belongs to mods, and \
             how many of them there are belongs to multiplayer whatever word the review uses \
             for them: \"the community is tiny\" and \"the playerbase is dead\" are multiplayer. \
             How the game disciplines them, bans, reports and moderators, belongs to policy.",
        ),
        alone: false,
    },
    Category {
        id: "updates",
        label: "Updates and developer support",
        description: "What the developers do after release. Patches, roadmaps, early access \
                      progress, communication with players, whether promises were kept, and \
                      whether the game is abandoned.",
        boundary: Some(
            "What the developers do to the game belongs here, and so does praise or blame \
             aimed at the studio itself, including thanking them and telling them to fix \
             things. A demand to fix one thing it names belongs to that thing, as a complaint \
             about it would: \"fix the accessibility\" is accessibility and \"fix the servers\" \
             multiplayer. \
             A complaint about what a patch changed is about the change: balance to \
             difficulty, a mechanic to gameplay, content removed to content. This is for the \
             patching itself, its pace, and whether they listen. A judgement about how the game \
             has changed since it came out is here even when no patch and no studio is named, \
             so \"it is getting better\", \"release was bad but it is fine now\" and \"one of \
             the worst launches in years\" are updates, where the same judgement with no before \
             and after, \"it is fine now\" alone, is a verdict. \"The game is dead\" is here \
             when nobody is developing it and multiplayer when nobody is playing it. What the \
             publisher or the platform requires of the player belongs to policy.",
        ),
        alone: false,
    },
    // Review bombs over publisher decisions are the reviews Steam's own default filter hides,
    // and this tool exists partly to count them. On one corpus measured they are 16% of
    // 1.16 million reviews, with nowhere to go: offtopic means a reader learns nothing about
    // the game, and "you need a second account to play" is not nothing.
    Category {
        id: "policy",
        label: "Publisher and platform policy",
        description: "Decisions taken by the publisher or the platform rather than by the \
                      game. Needing a second account to play, region locks and delistings, \
                      DRM and kernel-level anti-cheat, launcher requirements, price rises, \
                      and terms that changed after people had bought it.",
        boundary: Some(
            "Anything the publisher or the platform decided belongs here, including a protest \
             that is angry and brief and names no particular term, and including celebrating \
             one that was reversed: \"we won\" about a withdrawn account requirement is this, \
             and it is praise. What a decision is belongs here; what it does while playing \
             belongs where it happens, so a launcher that will not sign you in is here and an \
             anti-cheat that keeps you out of a match is multiplayer. How the game enforces its \
             rules on its players belongs here too: bans, reports, an anti-cheat verdict and \
             its appeal, and the moderators who apply them, including one who abuses the role. \
             What the players do to one another belongs to community. Only a protest about \
             something with no connection to this game at all belongs to offtopic.",
        ),
        alone: false,
    },
    Category {
        id: "compatibility",
        label: "Hardware and compatibility",
        description: "Whether it runs on your setup at all. System requirements, Steam Deck, \
                      Linux and Proton, ultrawide and multi-monitor support, drivers and \
                      hardware, and asking for it on a console or handheld.",
        boundary: Some(
            "Asking for a port to another platform belongs here, and so does a game that \
             will not start at all when no reason is given. A game that starts and then \
             crashes is bugs. Asking for a sequel belongs to verdict, and anything about \
             playing it in a headset belongs to VR. This row is about the machine: a \
             requirement the publisher chose, a second account, a launcher, a permanent \
             connection, is policy however completely it stops the game from running. A \
             reviewer naming the machine they played on and judging nothing, \"played on a \
             3070\", \"Steam Deck, forty hours\", is here and neutral: it tells a reader what \
             the review was tried on. A headset named that way is VR.",
        ),
        alone: false,
    },
    Category {
        id: "accessibility",
        label: "Accessibility and options",
        description: "A setting or feature put there so somebody can play who otherwise could \
                      not. Subtitles and their size, colourblind modes, remapping the controls \
                      and one-handed schemes, screen reader support, turning off screen shake \
                      or flashing, and a difficulty or assist option offered for the same \
                      reason.",
        boundary: Some(
            "This is the accommodation, never the thing it accommodates. Text being too small \
             to read is graphics, a game being too hard is difficulty, a control scheme that \
             does not respond is controls, and which languages the game is available in is \
             language; an option offered so that somebody can read it, beat it or play it \
             one-handed is here. The test is whether the claim is about something the game \
             provides on purpose for that reason: \"no subtitle size setting\" and \"the \
             colourblind mode is excellent\" are here, \"the subtitles are tiny\" is graphics. \
             Remapping the controls is here whatever it is wanted for. Any other option a \
             player needs is here; one that is a matter of taste, aim assist, a deadzone, the \
             sensitivity, is controls; one that is there and does not work is bugs. A \
             difficulty mode, a story mode included, is difficulty unless the claim names a \
             player who could not otherwise finish, a disability or a condition, and then it \
             is here, as are skipping a puzzle and slowing the game down; \"a wider audience\" \
             or \"most normal players\" wanting it easier is difficulty. The word on its own \
             decides nothing: a game \
             called accessible \
             because it is easy to pick up is tutorial, and one accessible in a country is \
             policy.",
        ),
        alone: false,
    },
    // Bundled into accessibility until it was measured: language complaints were most of
    // that category's mass while having nothing to do with accommodation, and they are the
    // single most common thing non-English reviews are about.
    Category {
        id: "language",
        label: "Language and localisation",
        description: "Which languages the game is in and how good the translation is. No \
                      English, no Chinese, machine translation, a language dropped in an \
                      update, subtitles only in some languages, playing anyway with a \
                      dictionary.",
        boundary: Some(
            "Whether a language exists and how well it reads belongs here. Subtitles as an \
             accommodation, in a language that is already supported, belong to accessibility.",
        ),
        alone: false,
    },
    // Asked for by three labellers independently across the earliest reference sets, which is the
    // strongest signal any reference set has produced. It was being split between gameplay
    // and difficulty, and "the tutorial explains nothing" is neither a mechanic nor a
    // question of balance.
    Category {
        id: "tutorial",
        label: "Tutorial and learning",
        description: "How the game teaches itself. The tutorial, the first hour, systems that \
                      are never explained, the learning curve, reading a wiki to understand \
                      anything, and whether a new player is left to work it out alone.",
        boundary: Some(
            "Whether the game explains itself belongs here. Whether it is hard once you do \
             understand it belongs to difficulty, and whether the systems themselves are any \
             good belongs to gameplay.",
        ),
        alone: false,
    },
    // A licence is most visible when it lapses: a team, a driver or a song that was in last
    // year's release and is not in this one. That had been landing in content, which is about
    // how much game there is, and in monetisation, which is about what is sold on top.
    Category {
        id: "licensing",
        label: "Licensed content",
        description: "Whether the game has the real names. Real teams, players, clubs and kits, \
                      real cars and tracks, licensed music, a licence lost or gained between \
                      releases, and how faithful the game is to the thing it is adapting.",
        boundary: Some(
            "A real name being present, missing or wrong belongs here, and that includes a \
             real team, league, competition or player that is absent, wrong or made generic, \
             \"no Liga MX\", \"the national teams are gone\", however it is phrased; content \
             keeps only a mode or feature that names nothing real. What is sold on top of the \
             game belongs to monetisation, whether an adaptation is well written belongs to \
             story, and a protest about a licence agreement, terms of service or an account \
             belongs to policy. Feeling as if you are inside the film, show or book the game \
             adapts is faithfulness said as a feeling, and belongs here. A licence withheld or \
             running out is here where the claim is about what the game lacks because of it, \
             \"the national teams are gone again\", and policy where it is about who holds the \
             licence or what they agreed, \"let somebody else have the NFL licence\".",
        ),
        alone: false,
    },
    // Kept out of compatibility, which is about whether a game runs at all. A review of a
    // headset game is about comfort, tracking and standing inside the thing, and none of that
    // is a system requirement.
    Category {
        id: "vr",
        label: "VR and headsets",
        description: "Playing it in a headset. Room scale and seated play, motion sickness and \
                      comfort options, tracking and the controllers in your hands, whether it \
                      was built for a headset or is a flat game bolted into one, and which \
                      headsets it works with.",
        boundary: Some(
            "Anything about being in a headset belongs here, including whether a given headset \
             is supported, and the tracked controllers that come with it, whether they are \
             detected and which ones work. What the game does with those controllers, the \
             scheme it binds to them and where it puts your hands, is controls. A headset that \
             worked and stopped after an update is bugs, and one whose support the publisher \
             withdrew is policy; this row keeps which headsets work, with nothing said about a \
             change. A game called immersive is not this, in a headset \
             or out of one: immersion with nothing named behind it is atmosphere, and \
             immersion a review explains belongs to what it names, so it is this only when \
             the headset itself is the reason, as in leaning over the table to look. How the \
             game looks on a monitor belongs to graphics.",
        ),
        alone: false,
    },
];

/// Text handed to the embedding model for a category. Boundary rules are deliberately absent.
#[must_use]
pub fn embedding_text(category: &Category) -> String {
    format!("{}. {}", category.label, category.description)
}

#[must_use]
pub fn by_id(id: &str) -> Option<&'static Category> {
    SHEET.iter().find(|c| c.id == id)
}

/// The category sheet handed to whoever is labelling, generated rather than retyped.
///
/// The first reference set was produced from instructions written by hand for each batch of
/// labellers, and they disagreed about the same boundary in near-identical cases because the
/// instructions never settled it. Generating the sheet from the taxonomy means a rule can
/// only be fixed in one place, and every labeller sees the same one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Unit {
    /// A whole review, which is what the first reference sets labelled.
    Review,
    /// One point a review makes. What the model is trained on, because a review is not one
    /// opinion and a vector for the whole of it is the average of the ones it holds.
    #[default]
    Claim,
}

#[must_use]
pub fn labelling_brief(unit: Unit) -> String {
    use std::fmt::Write as _;

    let mut brief = match unit {
        Unit::Review => String::from(
            "Categories. Each review gets exactly one primary \
             category, plus any others it genuinely also covers.\n\n\
             You are shown the text of a review and nothing else: not which game it is, not \
             whether the reviewer recommended it, not what the classifier guessed. The tool \
             being measured sorts reviews from their text alone, so a label made from more \
             than that would measure what you were told rather than how well it reads.\n\n",
        ),
        Unit::Claim => String::from(
            "Categories. You are labelling CLAIMS: the separate \
             points a review makes. Each claim gets exactly one subject.\n\n\
             A review arrives split into numbered claims, and you label every one of them. \
             The review is there so a claim like \"it doesn't\" or \"same here\" can be read \
             in context; the label is about the claim, not about the review around it. A \
             review that makes one point has one claim, and a review that makes twelve has \
             twelve, which is the whole reason for labelling this way: the tool used to \
             average a long review into a single vector and then file a two-word review under \
             four subjects.\n\n\
             You are not told which game it is, whether the reviewer recommended it, or what \
             the model guessed. The model reads the text alone, so a label made from more \
             than that measures what you were told rather than how well it reads.\n\n\
             Most claims name no aspect at all. \"Great game\", \"10/10\", \"gfg\", a row of \
             emoji: these are not graphics, not gameplay and not story, and filing them as \
             any of those is the exact failure this set exists to fix. They are `verdict` \
             when they judge the game and `offtopic` when they say nothing about it. Expect \
             to use those two more than anything else, and do not go looking for a subject \
             that is not there.\n\n",
        ),
    };
    for category in SHEET {
        let _ = writeln!(
            brief,
            "{} ({})\n  {}",
            category.label, category.id, category.description
        );
        if let Some(rule) = category.boundary {
            let _ = writeln!(brief, "  RULE: {rule}");
        }
        if category.alone && unit == Unit::Review {
            let _ = writeln!(
                brief,
                "  ONLY: this is the whole label. A review here covers nothing else, so it \
                 never takes a second category."
            );
        }
        brief.push('\n');
    }
    let fields = match unit {
        Unit::Review => FIELDS,
        Unit::Claim => CLAIM_FIELDS,
    };
    brief.push_str(
        &fields
            .replace(CONFIDENCE_SLOT, &CONFIDENCE.join(", "))
            .replace(POLARITY_SLOT, &POLARITY.join(", ")),
    );
    brief
}

/// The sheet a model works from when asked what a game's players talk about that the sheet
/// has no row for.
///
/// Generated from the sheet for the same reason the labelling sheet is: the reader has to know
/// every fixed subject to know what is not one, and a hand-written list of them would drift
/// the first time a subject was added.
#[must_use]
pub fn induction_brief() -> String {
    use std::fmt::Write as _;

    let mut brief = format!(
        "You are reading reviews of one game, chosen to be as unlike each other as the corpus \
         allows, and naming what its players talk about that no game shares.\n\n\
         Every game's reviews are sorted into the same {} subjects, \
         listed below. Those are the floor. What you are looking for is above it: a subject \
         this game's players return to that is not one of them, or is one of them in a form \
         so specific to this game that it deserves its own row. Mud physics in a truck \
         simulator. Jump scares in a horror game. Deck archetypes in a card game. The AI \
         opponent in a strategy game. Mods, where a game is played modded.\n\n\
         You are not looking for what people think of the game. Praise and complaint are \
         recorded elsewhere. You are looking for what they think about, and only what several \
         of them think about: a subject one review raises is an anecdote, and a subject you \
         cannot point at three reviews for does not exist.\n\n\
         The fixed subjects, which you must not name again:\n\n",
        SHEET.len()
    );
    for category in SHEET {
        let _ = writeln!(
            brief,
            "  {} ({}): {}",
            category.label, category.id, category.description
        );
    }
    brief.push_str(
        "\nReturn one JSON object and nothing else:\n\n\
         {\n  \"induced_by\": \"<the model you \
         are>\",\n  \"subjects\": [\n    {\n      \"id\": \"<short, lowercase, hyphens>\",\n      \
         \"label\": \"<what the row is called>\",\n      \"description\": \"<one sentence: \
         what belongs here and what does not>\",\n      \"refines\": \"<a fixed subject id, \
         or null>\",\n      \"evidence\": [\"<review_id>\", \"<review_id>\", \"<review_id>\"]\n    \
         }\n  ]\n}\n\n\
         `refines` names the fixed subject this one is a specific form of, when it is one, \
         and is null when it is not. `evidence` is review ids from the handout, as given, at \
         least three and every one of them a review that raises the subject. A subject whose \
         evidence names a review the handout did not hold is refused whole, and so is one \
         with fewer than three. Better to return four subjects that survive than twelve that \
         do not.\n\n\
         Between five and twelve subjects is the usual shape. Zero is a legitimate answer for \
         a game whose players talk about nothing the sheet does not already name, and is \
         better than an invented one.\n",
    );
    brief
}

/// The answers `confidence` may take.
///
/// A judgement asked for as free text drifts: "Medium", "fairly", "8/10", nothing at all. The
/// sheet asks for these three and ingest refuses anything else, from the same list, so the
/// column stays something a reader can count.
pub const CONFIDENCE: [&str; 3] = ["high", "medium", "low"];

/// Where the sheet names them, so the words are written down once.
const CONFIDENCE_SLOT: &str = "{confidence}";

/// The answers `polarity` may take.
///
/// Three rather than two, because a claim can state a fact about the game without judging it,
/// and forcing "it is a roguelike deckbuilder" to be praise or complaint would put a verdict
/// in a reviewer's mouth. Mixed is deliberately absent: a claim that both praises and
/// complains is two claims the splitter failed to separate, and `split_wrong` records that
/// instead of hiding it in a fourth value.
pub const POLARITY: [&str; 3] = ["praise", "complaint", "neutral"];

const POLARITY_SLOT: &str = "{polarity}";

/// What every claim label carries.
///
/// The review-level sheet asks for a primary and a secondary category. A claim takes exactly
/// one subject, and that is the point of the unit: where two genuinely fit, the split was
/// wrong, and saying so is worth more than a second category. `split_wrong` is how the
/// splitter gets measured by the people best placed to see it fail.
const CLAIM_FIELDS: &str = "\
Every claim label is six fields.

subject
  The one category this claim is about. Exactly one, always. Most claims name no aspect at
  all: those are `verdict` if they judge the game and `offtopic` if they do not.

polarity
  What the claim does about its subject, in one of these words: {polarity}. Praise and
  complaint are about the game, not about the reviewer's mood. Neutral is for a claim that
  states something without judging it, which is common and is not a failure to decide.

ironic
  The text says the opposite of what it appears to say. \"0/10, I have not slept in three
  days\" is praise; \"10/10 would lose my save file again\" is a complaint. Judge this from
  the words alone. You are not told whether the reviewer recommended the game, so that you
  cannot be led by it. Where a claim is ironic, `polarity` is what the reviewer MEANS.

confidence
  How sure you are of the subject, in one of these words: {confidence}. This one is about
  you, and it is used: the model is trained against your uncertainty rather than against a
  flattened guess, so a truthful \"low\" is worth more than a confident wrong answer.

ambiguous
  Whether the call is genuinely contested: two subjects fit and the rules above do not settle
  which. This is about the claim and the taxonomy rather than about you, and it is read back.
  Agreement is reported separately over the claims marked here.

split_wrong
  Whether this claim is really two points stuck together, or half of one that was cut in the
  wrong place. The splitting is mechanical and it will be wrong sometimes; this is the only
  signal that it was, and it is what improves it. Leave it false unless the text in front of
  you is genuinely mis-cut.

Return every one of these for every claim, in the order the claims are given. A judgement left
out is not a judgement, and a label missing one is refused rather than filled in with a guess.
";

/// What every label carries besides its categories.
///
/// Generated with the categories and for the same reason. The boundaries were written down
/// because labellers disagreed about them; these were left to whatever each batch was told,
/// and one of them decides how a headline figure is reported. `ambiguous` is what splits
/// agreement into clear-cut and contested, and across the earliest reference sets it marked between 21%
/// and 35% of a game depending on who labelled it, which is a spread no property of the
/// reviews explains.
///
/// `ironic` is a claim about the words, not about the rating. The labeller is never shown
/// whether the reviewer recommended the game, so that they cannot be led by it, which means
/// they cannot report that the two disagree either. They say what the text does; joining
/// that to the rating is arithmetic and belongs to the tool.
const FIELDS: &str = "\
Every label is five fields. The first two say which categories the review covers; the other
three are judgements about the review, and none of them changes which category it belongs to.

primary
  The one category the review is most about. Exactly one, always.

secondary
  Every other category the review genuinely also covers. Leave it empty when the review is
  about one thing. Never add a category to fill space.

ironic
  The text says the opposite of what it appears to say. \"0/10, I have not slept in three
  days\" is praise; \"10/10 would lose my save file again\" is a complaint. Judge this from
  the words alone. You are not told whether the reviewer recommended the game, so that you
  cannot be led by it.

confidence
  How sure you are of the primary category, in one of these words: {confidence}.
  This one is about you.

ambiguous
  Whether the call is genuinely contested: two categories fit and the rules above do not
  settle which. This one is about the review and the taxonomy rather than about you, and it
  is the judgement that is read back. Agreement is reported separately for the reviews
  marked here, because disagreement on them says as much about the taxonomy as about the
  classifier.

Return every one of these for every review. A judgement left out is not a judgement, and a
label missing one is refused rather than filled in with a guess: ironic and ambiguous are
claims about the review, and defaulting them to false would put words in your mouth on the
figure that decides how agreement is reported.
";

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn category_ids_are_unique_and_stable_looking() {
        let ids: HashSet<&str> = SHEET.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), SHEET.len(), "duplicate category id");
        for category in SHEET {
            assert!(
                category
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_'),
                "{} is not a stable-looking id",
                category.id
            );
        }
    }

    #[test]
    fn descriptions_are_written_as_sentences_not_labels() {
        for category in SHEET {
            assert!(
                category.description.len() > 60,
                "{} has too thin a description to embed usefully",
                category.id
            );
            assert!(
                category.description.ends_with('.'),
                "{} should read as prose",
                category.id
            );
        }
    }

    #[test]
    fn boundary_rules_never_reach_the_embedding_model() {
        // A rule is written to a labeller and reads like an instruction. Embedding one would
        // pull the anchor towards the language of instructions and away from reviews.
        for category in SHEET {
            let embedded = embedding_text(category);
            if let Some(rule) = category.boundary {
                assert!(
                    !embedded.contains(rule),
                    "{} embeds its boundary rule",
                    category.id
                );
            }
        }
    }

    #[test]
    fn every_boundary_rule_names_a_category_that_exists() {
        let ids: HashSet<&str> = SHEET.iter().map(|c| c.id).collect();
        for category in SHEET {
            let Some(rule) = category.boundary else {
                continue;
            };
            let named = ids
                .iter()
                .filter(|id| **id != category.id && rule.contains(*id))
                .count();
            assert!(
                named > 0,
                "{}'s rule resolves a conflict with nothing: {rule}",
                category.id
            );
        }
    }

    #[test]
    fn the_labelling_brief_carries_every_category_and_every_rule() {
        for unit in [Unit::Review, Unit::Claim] {
            let brief = labelling_brief(unit);
            for category in SHEET {
                assert!(brief.contains(category.id), "{} missing", category.id);
                assert!(brief.contains(category.description));
                if let Some(rule) = category.boundary {
                    assert!(brief.contains(rule), "{}'s rule missing", category.id);
                }
            }
        }
    }

    /// The claim sheet asks for one subject and a polarity, and the failure it exists to stop
    /// is a labeller hunting for a topic in "Great game".
    #[test]
    fn the_claim_brief_asks_for_one_subject_and_a_polarity() {
        let brief = labelling_brief(Unit::Claim);
        for field in [
            "subject",
            "polarity",
            "ironic",
            "confidence",
            "ambiguous",
            "split_wrong",
        ] {
            assert!(
                brief.contains(&format!("\n{field}\n")),
                "{field} goes unexplained"
            );
        }
        for word in POLARITY {
            assert!(
                brief.contains(word),
                "polarity {word} is asked for but not named"
            );
        }
        assert!(!brief.contains(POLARITY_SLOT));
        assert!(
            brief.contains("Return every one of these for every claim"),
            "the sheet does not say that every judgement is required"
        );
        assert!(
            !brief.contains("secondary"),
            "a claim takes one subject; offering a secondary invites the averaging this unit exists to remove"
        );
    }

    /// The judgements besides the categories were left to whatever each batch of labellers
    /// was told, and one of them decides how a headline figure is reported.
    #[test]
    fn the_brief_says_what_the_other_judgements_mean() {
        let brief = labelling_brief(Unit::Review);
        for field in ["primary", "secondary", "ironic", "confidence", "ambiguous"] {
            assert!(
                brief.contains(&format!("\n{field}\n")),
                "{field} goes unexplained"
            );
        }
        // The two are asked in the same breath and mean opposite things: one is a fact about
        // the labeller, the other a fact about the taxonomy. Only the second is read back.
        assert!(
            brief.contains("This one is about you.")
                && brief.contains("about the review and the taxonomy rather than about you"),
            "confidence and ambiguous are not told apart, which is the whole difficulty"
        );
        // Ingest drops a label that leaves one out, so the sheet is the only place a labeller
        // can find out that it will, and the only place the accepted words are written down.
        assert!(
            brief.contains("Return every one of these for every review."),
            "the sheet does not say that every judgement is required"
        );
        for word in CONFIDENCE {
            assert!(
                brief.contains(word),
                "the sheet asks for a confidence it does not name: {word}"
            );
        }
        assert!(
            !brief.contains(CONFIDENCE_SLOT),
            "the sheet still holds the placeholder instead of the words"
        );
    }

    #[test]
    fn the_checked_in_brief_is_the_one_this_build_would_generate() {
        // The sheet labellers work from is committed so it can be read without building the
        // tool, which makes it capable of drifting from the taxonomy it claims to describe.
        // A reference set labelled against a stale sheet is silently mislabelled, and the
        // first set produced by this project lost consistency exactly that way.
        for (file, unit) in [
            ("labelling-brief.txt", Unit::Review),
            ("claim-brief.txt", Unit::Claim),
        ] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../reference")
                .join(file);
            let committed = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} is missing: {e}", path.display()));
            assert_eq!(
                committed.replace("\r\n", "\n"),
                labelling_brief(unit),
                "reference/{file} is stale; regenerate it with `steamgauge brief`"
            );
        }
    }

    #[test]
    fn lookup_finds_categories_and_rejects_unknown_ones() {
        assert_eq!(by_id("bugs").map(|c| c.label), Some("Bugs and crashes"));
        assert_eq!(
            by_id("genre").map(|c| c.label),
            Some("Genre and comparisons")
        );
        assert!(by_id("not-a-category").is_none());
    }
}
