//! Fishing lines for the subjects the labelled set has almost none of.
//!
//! Eight of the twenty-six subjects have under 250 labels between them and `licensing` has
//! thirty-two. A random draw will not fix that: it lands on the same distribution the corpus
//! has, so another thousand random claims buys another two `vr` labels. Reweighting the loss
//! does not fix it either, because reweighting redistributes a gradient that thirty-two claims
//! do not contain; every weighting scheme tried here made macro F1 worse.
//!
//! What does fix it is going and finding them. Twenty million claims have been read and never
//! labelled, and the starved subjects are in there at their natural rate, which is small but
//! not zero: a corpus of seven million reviews holds tens of thousands of claims about
//! headsets.
//!
//! **A probe is a fishing line, not a definition.** It says where claims about a subject tend
//! to be found, which is a different thing from what the subject means: the taxonomy says what
//! `vr` is and a labeller decides whether a claim is one. Matching a probe only puts a claim
//! in front of the labeller. It follows that a probe may be loose, and several here are: the
//! cost of a false positive is one labelled claim that turns out to be `gameplay`, which is
//! still a labelled claim.
//!
//! **This is lexical, and lexical is the weaker half of the method.** The published form of
//! this selects by retrieval, embedding the pool and taking neighbours of the claims already
//! labelled, which finds the paraphrases no probe lists. Retrieval also travels across
//! languages, and these probes mostly do not: they lean on borrowed tokens that survive
//! translation ("VR", "Denuvo", "Proton", "mod") and on the one or two scripts where a term is
//! written the same way everywhere. A Russian review complaining about subtitles is not caught
//! here. That gap is the reason to build the retrieval pass, and it is not a reason to skip
//! this one: the borrowed tokens alone raise the hit rate on `vr` and `policy` by more than an
//! order of magnitude over a random draw, and they cost a scan rather than a GPU.
//!
//! **Point the draw at games that could plausibly hold the subject.** Two of these lines are
//! about a property of the game rather than a thing reviewers say: `licensing` fires on
//! adaptation vocabulary, and in a game that adapts nothing it catches film comparisons, which
//! are `genre`; `vr` fires on headset vocabulary, and a flat game's reviews mention headsets
//! only to say it has none. A run over a sports game, a tie-in and a headset game is worth
//! several over whatever happens to be captured.

use std::path::Path;

use crate::{Error, Result};

/// A claim a retrieval line caught: where it is, and how far it sat inside that line's side of
/// the margin.
type Near = (String, crate::claims::Span, f32);

/// Where claims about one starved subject tend to be found.
#[derive(Debug)]
pub struct Probe {
    /// Subject id in [`crate::taxonomy::SHEET`].
    pub subject: &'static str,
    /// Matched case-insensitively against the claim. An ASCII term must fall on a word
    /// boundary, so "mod" does not match "modern"; a term with non-ASCII characters matches
    /// as a bare substring, because the scripts that need it do not write word boundaries.
    pub terms: &'static [&'static str],
    /// A claim holding any of these is not this line's, however many terms it matches: the
    /// word the line fishes with also names something another row owns.
    pub unless: &'static [&'static str],
}

/// The subjects a word probe can find, and what to look for.
///
/// Ordered by how starved the subject was when each line was written, because the draw fills
/// its quotas in this order and a claim that matches two subjects is counted once, for the
/// first of them. The order is not a claim about today: `mods` and `compatibility` were among
/// the thinnest rows here and now score 0.74 and 0.79 on games the reader never saw, while
/// `licensing` sits at 0.49 and takes 1% of what these draws catch. Which rows are worth a
/// draw is a measurement, and `--only` is where it goes; the lines themselves are vocabulary
/// and stay whether or not a row needs them this week.
pub const PROBES: &[Probe] = &[
    Probe {
        // Real names, not licence agreements: a protest about an EULA is `policy`, and this
        // is the row that gets the sports and adaptation vocabulary.
        subject: "licensing",
        terms: &[
            // Not bare "licence": in a games corpus it is usually an in-game mechanic (a
            // mining licence, a pilot licence) or an end-user agreement, which is `policy`.
            "licensed",
            "unlicensed",
            "licensing",
            "official licence",
            "official license",
            "music licence",
            "music license",
            "lost the licence",
            "lost the license",
            "real names",
            "real teams",
            "real cars",
            // Not "real players": it means human opponents rather than bots, which is
            // `multiplayer`, and it did so in all 21 claims it caught, the football game's too.
            "fake names",
            "official teams",
            "fictional",
            "faithful to the",
            "adaptation",
            "the books",
            "the comics",
            "the anime",
            "the manga",
            "the movie",
            "the film",
            "the show",
            "canon",
            "lore accurate",
            "lore-accurate",
        ],
        unless: &[],
    },
    Probe {
        subject: "vr",
        terms: &[
            "vr",
            "pcvr",
            "steamvr",
            "openxr",
            "headset",
            "hmd",
            "oculus",
            "meta quest",
            "quest 2",
            "quest 3",
            // Not bare "vive": it is "long live" in French and Spanish, and a co-op game's
            // reviews are full of it.
            "htc vive",
            "the vive",
            "valve index",
            "psvr",
            "pimax",
            "pico 4",
            "wmr",
            "room scale",
            "roomscale",
            "room-scale",
            "6dof",
            "vr头显",
            "头显",
            // Not "motion sickness": a flat game induces it through camera shake and field of
            // view, which is `graphics` or `accessibility`, and DRG alone offered dozens.
        ],
        unless: &[],
    },
    Probe {
        subject: "accessibility",
        terms: &[
            // Not bare "accessible": in a review it nearly always means easy to get into,
            // which is `difficulty` or `verdict`, and it drowned this line when it was here.
            "accessibility option",
            "accessibility setting",
            "accessibility feature",
            "accessibility menu",
            "accessibility support",
            "colorblind",
            "colourblind",
            "color blind",
            "colour blind",
            "subtitle",
            "subtitles",
            "closed caption",
            "text size",
            "font size",
            "remap",
            "remappable",
            "rebind",
            "keybind",
            "key binding",
            "key bindings",
            "screen reader",
            "photosensitiv",
            "epilepsy",
            "epileptic",
            "one handed",
            "one-handed",
            "deaf players",
            "for the deaf",
            "hard of hearing",
            // Not bare "deaf": "tone deaf" is a complaint about a publisher, not a subtitle.
        ],
        unless: &[],
    },
    Probe {
        subject: "language",
        terms: &[
            "localization",
            "localisation",
            "localized",
            "localised",
            "translation",
            "translated",
            "mistranslat",
            "machine translat",
            "google translate",
            "no english",
            "english please",
            "english support",
            "add english",
            "中文",
            "简体",
            "繁體",
            "русский",
            "перевод",
            "日本語",
            "한국어",
            "português",
            "español",
            "deutsch",
            "français",
            "türkçe",
        ],
        unless: &[],
    },
    Probe {
        subject: "community",
        terms: &[
            "community",
            "playerbase",
            "player base",
            "the players are",
            "other players are",
            "toxic",
            "toxicity",
            "elitist",
            "gatekeep",
            "welcoming",
            "friendly community",
            "helpful community",
            "griefer",
            "griefing",
            "discord",
            "the forums",
            "steam forums",
            "subreddit",
        ],
        // "The community" is most often the modding one or the one the developers listen to,
        // and a labeller files those under `mods` and `updates`: on the first draw aimed at
        // this row, 15% of what this line caught was `community`. Over those three games these
        // terms turn away 75 claims, 34 `mods` and 28 `updates`, and not one `community`.
        unless: &[
            "mods",
            "modded",
            "modding",
            "modder",
            "moddable",
            "sdk",
            "workshop",
            "custom maps",
            "custom levels",
            "dev",
            "devs",
            "developer",
            "developers",
            "listen",
            "listens",
            "listened",
            "listening",
            "feedback",
        ],
    },
    Probe {
        subject: "mods",
        terms: &[
            // Not bare "mod", "модов" or "模组": a great many games call their own weapon
            // upgrades mods, and in Deep Rock Galactic that sense was most of what this line
            // caught. The forms below only name the user-made kind.
            "modded",
            "modding",
            "modder",
            "moddable",
            "workshop",
            "steam workshop",
            "nexusmods",
            "nexus mods",
            "script extender",
            "total conversion",
            "custom maps",
            "custom levels",
            "user created",
            "user-created",
            "мастерская",
            "创意工坊",
        ],
        unless: &[],
    },
    Probe {
        subject: "compatibility",
        terms: &[
            "steam deck",
            "steamdeck",
            "deck verified",
            "proton",
            "linux",
            "steamos",
            "macos",
            "mac os",
            "macbook",
            "ultrawide",
            "ultra wide",
            "21:9",
            "32:9",
            "multi monitor",
            "multi-monitor",
            "system requirements",
            "minimum requirements",
            "won't launch",
            "wont launch",
            "will not launch",
            "won't start",
            "wont start",
            "console port",
            "switch port",
            "handheld",
            "rog ally",
        ],
        unless: &[],
    },
    Probe {
        subject: "policy",
        terms: &[
            "denuvo",
            "drm",
            "anti-cheat",
            "anticheat",
            "anti cheat",
            "battleye",
            "easy anti-cheat",
            "kernel level",
            "kernel-level",
            "ring 0",
            // Not bare "launcher": a grenade launcher is not a games launcher, and in a
            // shooter it is most of what the word means.
            "game launcher",
            "another launcher",
            "separate launcher",
            "third party launcher",
            "third-party launcher",
            "launcher required",
            "requires a launcher",
            "epic games launcher",
            "epic games account",
            "ubisoft connect",
            "ea app",
            "rockstar social club",
            "second account",
            "separate account",
            "third party account",
            "region lock",
            "region-lock",
            "region locked",
            "delisted",
            "eula",
            "terms of service",
            "always online",
            "always-online",
            "requires an account",
        ],
        unless: &[],
    },
    Probe {
        // Not bare "sound" or "music": "sounds fun" is a verdict, "sounds like Dark Souls" is
        // genre, and "the music of the setting" is atmosphere. The words that are about the
        // audio itself carry a maker, a track or an ear with them.
        subject: "audio",
        terms: &[
            "soundtrack",
            "soundtracks",
            "ost",
            "bgm",
            // Not "score": in a games corpus it is the review score four times in six, and
            // the two that mean the music are already caught by "soundtrack".
            "sound design",
            "sound effects",
            "sound effect",
            "sfx",
            "voice acting",
            "voice actor",
            "voice actors",
            "voiceover",
            "voice over",
            "dub",
            "dubbing",
            "audio mixing",
            "audio bug",
            "音楽",
            "音效",
            "配音",
            "саундтрек",
            "озвучка",
            "musik",
            "vertonung",
            "banda sonora",
            "doblaje",
            "trilha sonora",
            "dublagem",
        ],
        unless: &[],
    },
    Probe {
        // The row is about being taught the game, so the line is the teaching and its absence.
        // Not "learning curve": how hard a game is to learn is difficulty by the sheet's rule,
        // and the curve is the commonest way to say it.
        subject: "tutorial",
        terms: &[
            "tutorial",
            "tutorials",
            "tutoriel",
            "tutoriales",
            "onboarding",
            "no explanation",
            "never explains",
            "does not explain",
            "doesn't explain",
            "explains nothing",
            "without explaining",
            "figure it out yourself",
            "figure out how",
            "wiki to play",
            "read the wiki",
            "チュートリアル",
            "教程",
            "新手教学",
            "튜토리얼",
            "обучение",
            "туториал",
            "einführung",
            "erklärt nichts",
        ],
        unless: &[],
    },
];

/// Which subject's line a claim took, if any.
///
/// First match wins, in [`PROBES`] order, so the most starved subject gets the claim when two
/// lines cross. A claim saying "the VR mod" is offered as `vr` and not as `mods`, which is the
/// right way round: `vr` has forty-one labels and `mods` has a hundred and forty-six.
#[must_use]
pub fn hooked(claim: &str) -> Option<&'static str> {
    hooked_among(claim, &[])
}

/// The same, restricted to the subjects named, or every probe when none are.
///
/// Which rows are worth fishing for is a measurement and it moves: `mods` and `compatibility`
/// were among the thinnest rows when these lines were written and now score 0.74 and 0.79 on
/// games the reader never saw, while `licensing` sits at 0.49 and takes 1% of what the draws
/// catch. Deleting their lines would throw away the vocabulary; naming the rows per draw keeps
/// it and puts the aiming where the evidence is.
#[must_use]
pub fn hooked_among(claim: &str, only: &[String]) -> Option<&'static str> {
    let haystack = claim.to_lowercase();
    PROBES
        .iter()
        .filter(|probe| only.is_empty() || only.iter().any(|name| name == probe.subject))
        .find(|probe| {
            probe
                .terms
                .iter()
                .any(|term| contains_term(&haystack, term))
                && !probe
                    .unless
                    .iter()
                    .any(|term| contains_term(&haystack, term))
        })
        .map(|probe| probe.subject)
}

/// Whether a lowercased claim holds this term, on a word boundary when the term is ASCII.
///
/// Without the boundary "mod" matches "modern" and "model", and "vr" matches nothing useful at
/// all in a language that happens to spell a common word with those two letters together.
fn contains_term(haystack: &str, term: &str) -> bool {
    if !term.is_ascii() {
        return haystack.contains(term);
    }
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(at) = haystack[from..].find(term) {
        let start = from + at;
        let end = start + term.len();
        let before_is_word = start > 0 && is_word_byte(bytes[start - 1]);
        let after_is_word = end < bytes.len() && is_word_byte(bytes[end]);
        if !before_is_word && !after_is_word {
            return true;
        }
        // Advance by one byte rather than by the term, so overlapping positions are seen; the
        // haystack is lowercase ASCII-searchable but may hold multi-byte characters, and
        // `find` returns a char boundary, so stepping one byte and letting `find` resynchronise
        // is safe.
        from = start + 1;
        if from >= haystack.len() {
            break;
        }
    }
    false
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

/// Every labelled claim, embedded, to fish with.
///
/// This is the other half of the method, and the stronger one. A probe lists words; a labelled
/// `policy` claim about Denuvo sits near every other complaint about copy protection whatever
/// words it uses and whatever language it is in, and a Russian review complaining about
/// subtitles is near an English one. The published form of this selects by retrieval for
/// exactly that reason ([arXiv:2307.14899](https://arxiv.org/pdf/2307.14899)).
///
/// Every labelled claim is a query, not a centroid of them. `policy` spans DRM, region locks,
/// account requirements and delistings, and the mean of those points is near none of them.
///
/// **The common subjects' claims are queries too, and they are what makes this work.** Fishing
/// with the starved subjects alone was tried first and a quarter of what it caught was "great
/// game": a short generic claim sits near every short claim, so one short query on a line
/// pulls in every short claim in the corpus. What a claim is nearest to among the starved
/// subjects is not the question. The question is whether it is nearer a starved subject than
/// it is to `verdict` or `gameplay`, and answering that needs `verdict` and `gameplay` in the
/// water as well. That is a nearest-neighbour vote, with every labelled claim voting.
#[derive(Debug)]
pub struct Lines {
    /// Subject id, one per probe, in [`PROBES`] order.
    subjects: Vec<&'static str>,
    /// Every query's unit vector, one row each.
    matrix: ndarray::Array2<f32>,
    /// Which line a query belongs to, or `None` for a query of a common subject.
    owner: Vec<Option<usize>>,
}

impl Lines {
    /// Embeds every labelled claim under `reference_root`.
    ///
    /// # Errors
    ///
    /// Fails if the reference sets cannot be read or the forward pass fails.
    /// `only` narrows the lines to some of the starved subjects. The others do not stop
    /// voting: they join the common subjects on the far side of the margin, so a draw aimed at
    /// `licensing` alone keeps `policy` claims about licence agreements out of it. A draw for
    /// one row is how a row that no game picked for the others will fill gets filled, which
    /// is what `licensing` needed after ten games of retrieval gave it sixteen labels.
    pub fn cast(
        embedder: &mut crate::embed::Embedder,
        reference_root: &Path,
        batch_size: usize,
        only: Option<&[String]>,
    ) -> Result<Self> {
        let subjects: Vec<&'static str> = PROBES
            .iter()
            .map(|probe| probe.subject)
            .filter(|subject| only.is_none_or(|wanted| wanted.iter().any(|w| w == subject)))
            .collect();
        if subjects.is_empty() {
            return Err(Error::NoReferenceSet {
                path: reference_root.to_path_buf(),
            });
        }
        let labelled = crate::claimset::labelled_claims(reference_root)?;

        let mut texts: Vec<String> = Vec::new();
        let mut owner: Vec<Option<usize>> = Vec::new();
        for claim in labelled {
            // A claim the labeller called contested is one two subjects fit, and a vote from
            // it is a vote for both.
            if claim.label.ambiguous || claim.text.trim().is_empty() {
                continue;
            }
            owner.push(
                subjects
                    .iter()
                    .position(|subject| *subject == claim.label.subject),
            );
            texts.push(claim.text);
        }
        if texts.is_empty() || owner.iter().all(Option::is_none) {
            return Err(Error::NoReferenceSet {
                path: reference_root.to_path_buf(),
            });
        }

        let mut rows: Vec<f32> = Vec::new();
        let mut dimensions = 0;
        for chunk in texts.chunks(batch_size.max(1)) {
            for vector in embedder.embed(chunk)? {
                dimensions = vector.len();
                rows.extend(vector);
            }
        }
        let matrix = ndarray::Array2::from_shape_vec((texts.len(), dimensions), rows)?;
        Ok(Self {
            subjects,
            matrix,
            owner,
        })
    }

    /// How many queries each line holds, in [`PROBES`] order, and how many vote against.
    #[must_use]
    pub fn cast_count(&self) -> (Vec<(&'static str, usize)>, usize) {
        let mut counts = vec![0; self.subjects.len()];
        let mut against = 0;
        for owner in &self.owner {
            match owner {
                Some(line) => counts[*line] += 1,
                None => against += 1,
            }
        }
        (self.subjects.iter().copied().zip(counts).collect(), against)
    }

    /// For each claim in a batch: the line it sits nearest, and by what margin over the
    /// nearest common subject.
    ///
    /// The margin is the figure. Positive means the claim is nearer some labelled claim of
    /// that starved subject than it is to any labelled claim of any common one, which is what
    /// "looks like a `vr` claim" has to mean when `verdict` is a fifth of the corpus.
    ///
    /// A large game is three million claims against twenty thousand queries, forty-five
    /// teraflops, and the card is busy embedding. Single-threaded that product took longer
    /// than the forward pass it followed, so the batch is cut across every core and each
    /// slice takes its own product against the whole query matrix.
    fn margins(&self, batch: &[Vec<f32>]) -> Result<Vec<Option<(usize, f32)>>> {
        if batch.is_empty() {
            return Ok(Vec::new());
        }
        let dimensions = self.matrix.ncols();
        let flat: Vec<f32> = batch.iter().flat_map(|row| row.iter().copied()).collect();
        let claims = ndarray::Array2::from_shape_vec((batch.len(), dimensions), flat)?;

        let workers = std::thread::available_parallelism().map_or(1, usize::from);
        let slice = batch.len().div_ceil(workers).max(1);
        let found: Vec<Vec<Option<(usize, f32)>>> = std::thread::scope(|scope| {
            let handles: Vec<_> = claims
                .axis_chunks_iter(ndarray::Axis(0), slice)
                .map(|rows| scope.spawn(move || self.margins_of(&rows)))
                .collect();
            // A slice that panicked would leave the batch's answers misaligned with its
            // claims, and every claim after it labelled by the wrong margin. A crash is the
            // honest outcome.
            handles
                .into_iter()
                .map(|handle| handle.join().expect("a margin slice panicked"))
                .collect()
        });
        Ok(found.into_iter().flatten().collect())
    }

    fn margins_of(&self, claims: &ndarray::ArrayView2<'_, f32>) -> Vec<Option<(usize, f32)>> {
        let similarities = claims.dot(&self.matrix.t());
        similarities
            .rows()
            .into_iter()
            .map(|row| {
                let mut best_line: Vec<f32> = vec![f32::NEG_INFINITY; self.subjects.len()];
                let mut best_common = f32::NEG_INFINITY;
                for (similarity, owner) in row.iter().zip(&self.owner) {
                    match owner {
                        Some(line) => best_line[*line] = best_line[*line].max(*similarity),
                        None => best_common = best_common.max(*similarity),
                    }
                }
                best_line
                    .iter()
                    .enumerate()
                    .filter(|(_, near)| near.is_finite())
                    .max_by(|(_, a), (_, b)| a.total_cmp(b))
                    .map(|(line, near)| (line, near - best_common))
            })
            .collect()
    }
}

/// What a retrieval draw found: the claims, and by what margin each line's catch was caught.
#[derive(Debug)]
pub struct Retrieved {
    pub drawn: Vec<crate::claimset::DrawnReview>,
    /// Per line: how many were taken, the widest margin and the narrowest, in [`PROBES`]
    /// order. The narrowest is the figure to read. A margin is how much nearer a claim sits
    /// to the line's labelled claims than to any common subject's, so a line whose
    /// two-hundredth catch has a negative one was scraping the floor of the corpus for a
    /// subject it does not hold, and those labels will mostly say `gameplay`.
    pub by_line: Vec<(&'static str, usize, f32, f32)>,
    pub claims_seen: u64,
}

/// Draws the claims that sit nearer a starved subject's labelled claims than any common one's.
///
/// Embeds the corpus as it goes rather than reading stored vectors, for two reasons. Stored
/// claim vectors exist for no game yet, and the splitter changes: a vector file cut by one
/// splitter names claims another does not, which is the failure `check-draws` exists to catch,
/// and a draw that re-embeds cannot suffer it. The cost is a forward pass over the corpus per
/// draw, minutes on a card for a large game, which a draw of two hundred claims can afford.
///
/// A claim goes to the starved line it sits nearest, keyed by its margin over the nearest
/// common subject, and each line keeps its widest `wanted`. There is deliberately no floor on
/// the margin: a floor that suits `vr` starves `licensing`, and the per-line narrowest margin
/// in [`Retrieved::by_line`] is what says whether a line was worth casting in this game.
///
/// Every review carries `subset: "retrieved"`, and no prevalence figure may count one.
///
/// # Errors
///
/// Fails if there is no capture, no reading, the reading was cut by another splitter, or the
/// forward pass fails.
#[expect(
    clippy::too_many_arguments,
    reason = "a draw is its corpus, its reference set and its budget, and each is a separate thing"
)]
pub fn draw_by_neighbour(
    embedder: &mut crate::embed::Embedder,
    lines: &Lines,
    out_dir: &Path,
    app_id: u32,
    dir: &Path,
    wanted: usize,
    batch_size: usize,
    mut on_progress: impl FnMut(u64),
) -> Result<Retrieved> {
    let snapshot = crate::embed::latest_snapshot(out_dir, app_id)?;
    let reading: crate::read::ReadReport =
        serde_json::from_slice(&std::fs::read(snapshot.join("reading.json")).map_err(|_| {
            crate::Error::NoClassifications {
                path: snapshot.join("reading.json"),
            }
        })?)?;
    let depth = reading.depth;

    let already = crate::claimset::already_drawn(dir);

    // The key is the margin turned upside down and quantised, so the bounded keeper's
    // "smallest" is "widest".
    let mut kept: Vec<crate::bounded::Smallest<u32, Near>> = lines
        .subjects
        .iter()
        .map(|_| crate::bounded::Smallest::new(wanted))
        .collect();
    let mut claims_seen = 0_u64;

    // Claims are gathered by the thousand and embedded shortest first, the way the embedding
    // pass does. A batch is padded to its longest member, and in review order every batch
    // holds one wall of text that pads two hundred and fifty short claims to five hundred
    // tokens: measured at three hundred and fifty claims a second before this, which made a
    // large game two hours.
    let window = batch_size.saturating_mul(64).max(batch_size);
    let mut pending: Vec<(String, crate::claims::Span, String)> = Vec::with_capacity(window);
    let mut flush = |pending: &mut Vec<(String, crate::claims::Span, String)>,
                     kept: &mut Vec<crate::bounded::Smallest<u32, Near>>|
     -> Result<()> {
        if pending.is_empty() {
            return Ok(());
        }
        pending.sort_unstable_by_key(|(_, _, text)| text.len());
        for chunk in pending.chunks(batch_size.max(1)) {
            let texts: Vec<String> = chunk.iter().map(|(_, _, text)| text.clone()).collect();
            let vectors = embedder.embed(&texts)?;
            let margins = lines.margins(&vectors)?;
            for ((id, at, _), found) in chunk.iter().zip(margins) {
                let Some((line, margin)) = found else {
                    continue;
                };
                kept[line].offer(margin_key(margin), (id.clone(), *at, margin));
            }
        }
        pending.clear();
        Ok(())
    };

    crate::capture::for_each_body(&snapshot, |id, _, text| {
        if already.contains(id) {
            return Ok(());
        }
        for (claim, span) in depth.claims_of(text).into_iter().zip(depth.spans_of(text)) {
            let trimmed = claim.trim();
            if trimmed.is_empty() {
                continue;
            }
            claims_seen += 1;
            pending.push((
                id.to_owned(),
                (
                    u32::try_from(span.start).unwrap_or(u32::MAX),
                    u32::try_from(span.end).unwrap_or(u32::MAX),
                ),
                trimmed.to_owned(),
            ));
            if pending.len() >= window {
                flush(&mut pending, &mut kept)?;
                on_progress(claims_seen);
            }
        }
        Ok(())
    })?;
    flush(&mut pending, &mut kept)?;
    on_progress(claims_seen);

    let caught: Vec<Vec<Near>> = kept
        .into_iter()
        .map(crate::bounded::Smallest::take)
        .collect();
    let by_line = lines
        .subjects
        .iter()
        .zip(&caught)
        .map(|(subject, line)| {
            let nearest = line.first().map_or(0.0, |(_, _, s)| *s);
            let furthest = line.last().map_or(0.0, |(_, _, s)| *s);
            (*subject, line.len(), nearest, furthest)
        })
        .collect();
    let picks: Vec<Vec<(String, (u32, u32))>> = caught
        .into_iter()
        .map(|line| line.into_iter().map(|(id, at, _)| (id, at)).collect())
        .collect();
    let (picks, _) = crate::claimset::round_robin(&picks, wanted);
    Ok(Retrieved {
        drawn: crate::claimset::handouts(&snapshot, app_id, depth, &picks, "retrieved")?,
        by_line,
        claims_seen,
    })
}

/// A margin as a key the bounded keeper sorts ascending, widest first.
///
/// A margin is a difference of two cosines and lies in [-2, 2]. Quantised to a millionth,
/// which is finer than an fp16 forward pass can tell two claims apart by, so nothing is lost
/// in the ordering.
fn margin_key(margin: f32) -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped to [0, 4] and scaled to fit"
    )]
    let key = ((2.0 - margin.clamp(-2.0, 2.0)) * 1_000_000.0) as u32;
    key
}

#[cfg(test)]
mod tests {
    use super::{PROBES, hooked, hooked_among};

    /// A term listed twice is a term that was edited twice and reconciled neither time, and
    /// nothing else in the file would notice.
    #[test]
    fn no_term_is_listed_twice() {
        let mut seen = std::collections::HashSet::new();
        for probe in PROBES {
            for term in probe.terms {
                assert!(seen.insert(*term), "{term:?} is listed twice");
                assert_eq!(
                    *term,
                    term.to_lowercase(),
                    "{term:?} is matched against lowercase and will never fire"
                );
            }
        }
    }

    #[test]
    fn every_probe_names_a_subject_the_sheet_has() {
        for probe in PROBES {
            assert!(
                crate::taxonomy::SHEET
                    .iter()
                    .any(|category| category.id == probe.subject),
                "{} is not a subject",
                probe.subject
            );
        }
    }

    #[test]
    fn a_term_inside_a_longer_word_is_not_a_match() {
        assert_eq!(hooked("a thoroughly modern shooter"), None);
        assert_eq!(hooked("the workshopping of ideas"), None);
        assert_eq!(hooked("the steam workshop is full of them"), Some("mods"));
    }

    #[test]
    fn the_starved_subject_wins_a_claim_that_matches_two() {
        assert_eq!(hooked("the VR modding scene is incredible"), Some("vr"));
    }

    #[test]
    fn a_draw_aimed_at_one_row_casts_that_line_and_not_the_ones_above_it() {
        let only = |name: &str| vec![name.to_owned()];
        // `vr` comes before `mods` in the list, so without narrowing it takes the claim.
        assert_eq!(
            hooked_among("the VR modding scene is incredible", &only("mods")),
            Some("mods")
        );
        // A claim only another line catches is not offered under the row that was asked for.
        assert_eq!(
            hooked_among("no Steam Deck support", &only("licensing")),
            None
        );
        assert_eq!(
            hooked_among("no Steam Deck support", &[]),
            Some("compatibility")
        );
    }

    #[test]
    fn the_modding_community_is_not_offered_as_community() {
        let only = |name: &str| vec![name.to_owned()];
        assert_eq!(
            hooked_among(
                "cant wait for the modding community to take it somewhere",
                &only("community")
            ),
            None
        );
        assert_eq!(
            hooked_among(
                "I hope that the community makes some cool mods",
                &only("community")
            ),
            None
        );
        assert_eq!(hooked("the modding community keeps it alive"), Some("mods"));
        assert_eq!(
            hooked_among("the devs never listen to the community", &only("community")),
            None
        );
        assert_eq!(
            hooked_among("the community is toxic and elitist", &only("community")),
            Some("community")
        );
    }

    #[test]
    fn a_term_that_is_not_ascii_matches_without_a_boundary() {
        assert_eq!(hooked("求求你们加个中文吧"), Some("language"));
        assert_eq!(hooked("创意工坊的模组很多"), Some("mods"));
    }

    /// Each of these was caught by a probe that has since been narrowed, and each was the
    /// commonest thing its line returned rather than a curiosity: a shooter says "launcher"
    /// about a weapon, a game with an upgrade tree says "mods" about it, "accessible" is how
    /// reviewers say a game is easy to get into, and a publisher is called tone deaf.
    #[test]
    fn the_wrong_sense_of_a_word_does_not_take_a_line() {
        assert_eq!(hooked("the grenade launcher is devastating"), None);
        assert_eq!(hooked("interesting weapon mods to unlock"), None);
        assert_eq!(hooked("I found it entertaining and accessible"), None);
        assert_eq!(hooked("way more fun against real players than bots"), None);
        assert_eq!(hooked("a tone deaf announcement from the publisher"), None);
        assert_eq!(hooked("upgrade your bear license, drink beer"), None);
        assert_eq!(hooked("vive la DRG, longue vie a eux"), None);
        // "sounds like" is a comparison and "the score" is the review's, four times in six.
        assert_eq!(hooked("it sounds like Dark Souls with guns"), None);
        assert_eq!(hooked("its 97% review score is here for a reason"), None);
        // How hard a game is to learn is difficulty by the sheet's rule, not tutorial.
        assert_eq!(hooked("the learning curve is brutal"), None);
    }

    #[test]
    fn the_two_rows_that_joined_the_list_have_lines_that_reach_them() {
        assert_eq!(hooked("the soundtrack is unbelievable"), Some("audio"));
        assert_eq!(
            hooked("the voice acting carries the whole thing"),
            Some("audio")
        );
        assert_eq!(hooked("音楽が最高"), Some("audio"));
        assert_eq!(hooked("the tutorial teaches you nothing"), Some("tutorial"));
        assert_eq!(
            hooked("the game never explains its systems"),
            Some("tutorial")
        );
        assert_eq!(
            hooked("you have to read the wiki to play it"),
            Some("tutorial")
        );
    }

    #[test]
    fn punctuation_and_case_do_not_hide_a_term() {
        assert_eq!(hooked("(VR) is unplayable"), Some("vr"));
        assert_eq!(hooked("DENUVO. again."), Some("policy"));
        assert_eq!(hooked("no Steam Deck support"), Some("compatibility"));
    }

    #[test]
    fn a_claim_about_nothing_on_the_list_is_not_hooked() {
        assert_eq!(hooked("the combat feels weightless"), None);
    }
}
