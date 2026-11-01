//! The app as a Model Context Protocol server: every tool is something the window can do, called
//! through the same function the window's command is, so a client and a person can never be
//! offered different things. A client reaches it through `steamgauge mcp` (see `crate::mcp`),
//! and only while the window is open: closing the window closes the app, clients or not.

use std::{future::Future, io, path::Path, pin::Pin, sync::LazyLock};

use base64::Engine as _;
use interprocess::local_socket::{
    ListenerOptions,
    tokio::{Listener, Stream},
    traits::tokio::Listener as _,
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    handler::server::common::schema_for_type,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock,
        GetPromptRequestParams, GetPromptResponse, Implementation, JsonObject, ListPromptsResult,
        ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
        ToolAnnotations,
    },
    service::RequestContext,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tauri::{AppHandle, Emitter, Manager};

use super::{
    LastSearch, Models, cockpit, data_export, game_updates, newer, settings, since, storage, text,
    update, who, work,
};
use crate::mcp::{rendezvous, socket_name};

/// What a client is told the server is for, before it has looked at a single tool.
const INSTRUCTIONS: &str = "SteamGauge downloads every Steam review of a game and reads each \
claim in them with a model on this machine, then shows what players praise and criticise, with \
the reviews behind every figure. Each tool here is something the SteamGauge window can do, and \
the person sees its effects there. A game is named by its Steam app id: find_games looks one up \
by name. Downloads, reads, updates and exports are jobs: queue starts them and returns at once, \
work shows where each stands. A game can be read only after its download finishes. Once it is \
read, reading gives its subjects and figures, and claims_behind and search_game the reviews \
behind them. show puts a \
page in front of the person. SteamGauge answers only while its window is open. When a tool says it \
is not open, ask the person to open it rather than trying to start it.";

/// What a tool does to the app, which tells a client how carefully to call it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effect {
    /// Only looks: nothing on disk or in the window changes.
    Looks,
    /// Starts work or changes what is kept, and nothing is lost by it.
    Changes,
    /// Removes something that has to be downloaded or worked out again to get back.
    Removes,
}

impl Effect {
    fn annotations(self) -> ToolAnnotations {
        ToolAnnotations::new()
            .read_only(self == Self::Looks)
            .destructive(self == Self::Removes)
    }
}

type Answer = Pin<Box<dyn Future<Output = Result<Vec<ContentBlock>, String>> + Send>>;

struct Entry {
    tool: Tool,
    effect: Effect,
    call: Box<dyn Fn(AppHandle, JsonObject) -> Answer + Send + Sync>,
}

/// A tool whose answer is given as it is.
fn answering<A, F, Fut>(
    name: &'static str,
    effect: Effect,
    description: &'static str,
    f: F,
) -> Entry
where
    A: DeserializeOwned + JsonSchema + Send + 'static,
    F: Fn(AppHandle, A) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Vec<ContentBlock>, String>> + Send + 'static,
{
    Entry {
        tool: Tool::new(name, description, schema_for_type::<A>())
            .with_annotations(effect.annotations()),
        effect,
        call: Box::new(move |app, arguments| {
            match serde_json::from_value::<A>(serde_json::Value::Object(arguments)) {
                Ok(arguments) => Box::pin(f(app, arguments)),
                Err(error) => Box::pin(std::future::ready(Err(format!(
                    "the arguments do not fit {name}: {error}"
                )))),
            }
        }),
    }
}

/// A tool whose answer is what the window's command returns, as JSON.
fn tool<A, R, F, Fut>(name: &'static str, effect: Effect, description: &'static str, f: F) -> Entry
where
    A: DeserializeOwned + JsonSchema + Send + 'static,
    R: Serialize + 'static,
    F: Fn(AppHandle, A) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<R, String>> + Send + 'static,
{
    answering(name, effect, description, move |app, arguments| {
        let answer = f(app, arguments);
        async move {
            let answer = answer.await?;
            ContentBlock::json(answer)
                .map(|block| vec![block])
                .map_err(|error| error.message.into_owned())
        }
    })
}

/// A window command that works on the window's thread, run on a blocking one here so a client's
/// call never holds up the server's.
async fn off_thread<R: Send + 'static>(
    work: impl FnOnce() -> R + Send + 'static,
) -> Result<R, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(text)
}

/// As [`off_thread`], for a command that can fail.
async fn off_thread_trying<R: Send + 'static>(
    work: impl FnOnce() -> Result<R, String> + Send + 'static,
) -> Result<R, String> {
    off_thread(work).await?
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Nothing {}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OneClient {
    /// The client's id, as `settings` lists it under `clients`.
    client: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OneGame {
    /// The game's Steam app id.
    app_id: u32,
}

/// What `reading` gives of a game. The whole game page is some 70,000 characters on a game with
/// many themes, more than a client can take in one answer, so the parts a question about the
/// game seldom needs come only when asked for.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadingAsked {
    /// The game's Steam app id.
    app_id: u32,
    /// How many themes on each side, the most widely said first, 10 by default.
    /// `claims_behind` with a theme gives the reviews behind one.
    themes: Option<usize>,
    /// Also how the figures move month by month.
    #[serde(default)]
    months: bool,
    /// Also the kinds of reviewer, and where each differs from the rest.
    #[serde(default)]
    who: bool,
    /// Also the languages the reviews are written in.
    #[serde(default)]
    languages: bool,
}

/// How many themes a side `reading` gives when not asked for a number.
const THEMES_GIVEN: usize = 10;

/// A reading cut to what was asked for.
fn as_asked(found: &impl Serialize, asked: &ReadingAsked) -> Result<serde_json::Value, String> {
    let mut reading = serde_json::to_value(found).map_err(text)?;
    let Some(fields) = reading.as_object_mut() else {
        return Ok(reading);
    };
    if let Some(sides) = fields
        .get_mut("themes")
        .and_then(serde_json::Value::as_object_mut)
    {
        for side in sides.values_mut() {
            if let Some(themes) = side.as_array_mut() {
                themes.truncate(asked.themes.unwrap_or(THEMES_GIVEN));
            }
        }
    }
    for (part, wanted) in [
        ("months", asked.months),
        ("who", asked.who),
        ("languages", asked.languages),
    ] {
        if !wanted {
            fields.remove(part);
        }
    }
    Ok(reading)
}

/// What `work` gives of the board. Two hundred queued jobs come to some 60,000 characters, more
/// than a client can take in one answer, so a state with many jobs gives its nearest few.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WorkAsked {
    /// Only the jobs about this game, every one of them.
    app_id: Option<u32>,
    /// How many jobs of each other state: the next queued, the latest ended. The default is 10.
    jobs: Option<usize>,
}

/// How many jobs of each state `work` gives when not asked for a number.
const JOBS_GIVEN: usize = 10;

/// The board cut to what was asked for, with how many of each state were left out.
fn work_as_asked(board: Vec<work::Job>, asked: &WorkAsked) -> serde_json::Value {
    let board: Vec<work::Job> = board
        .into_iter()
        .filter(|job| {
            asked
                .app_id
                .is_none_or(|app_id| job.task.app_id() == Some(app_id))
        })
        .collect();
    let given = if asked.app_id.is_some() {
        usize::MAX
    } else {
        asked.jobs.unwrap_or(JOBS_GIVEN)
    };
    let mut jobs = Vec::new();
    let mut left_out = serde_json::Map::new();
    for state in [
        work::State::Running,
        work::State::Queued,
        work::State::Failed,
        work::State::Stopped,
        work::State::Done,
    ] {
        let mut these: Vec<&work::Job> = board.iter().filter(|job| job.state == state).collect();
        let keep = if state == work::State::Running {
            these.len()
        } else {
            given
        };
        if state != work::State::Queued {
            these.sort_by_key(|job| std::cmp::Reverse(job.ended));
        }
        if these.len() > keep {
            left_out.insert(
                serde_json::to_value(state)
                    .ok()
                    .and_then(|name| name.as_str().map(str::to_owned))
                    .unwrap_or_default(),
                (these.len() - keep).into(),
            );
            these.truncate(keep);
        }
        jobs.extend(these.into_iter().cloned());
    }
    serde_json::json!({ "jobs": jobs, "left_out": left_out })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SomeGames {
    /// Steam app ids.
    app_ids: Vec<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MaybeGame {
    /// A game's Steam app id, or none.
    app_id: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MaybeGames {
    /// Steam app ids, or none for every game in the library.
    app_ids: Option<Vec<u32>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Words {
    /// A game's name, or part of it.
    words: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OneJob {
    /// The job's id, from `queue` or `work`.
    id: u64,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Tasks {
    /// The jobs to start. Jobs that share the network, the graphics card or the disk run one
    /// after another, in the order given.
    tasks: Vec<work::Task>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct WhoWrote {
    /// The game's Steam app id.
    app_id: u32,
    /// The kind of reviewer to look at, one of the kinds `reading` lists under `who`.
    these: String,
    /// The kind to set them beside, or none for everyone else.
    others: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ClaimsBehind {
    /// The game's Steam app id.
    app_id: u32,
    /// The subject's id, as `reading` gives it.
    subject: String,
    /// "praise", "complaint" or "neutral", for one side only.
    side: Option<String>,
    /// Only the claims using this term, as `reading` lists under the subject.
    term: Option<String>,
    /// Only one kind of reviewer's claims.
    who: Option<String>,
    /// Only the claims of one theme, by its place among the subject's themes on `side`, as
    /// `reading` gives it.
    theme: Option<usize>,
    /// How many claims to skip, for the next page.
    from: usize,
    /// How many claims to give.
    count: usize,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReadOffer {
    /// The game to price a read of, or none for a typical game.
    app_id: Option<u32>,
    /// The language to read, or none for every language.
    language: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SearchGame {
    /// The game's Steam app id.
    app_id: u32,
    /// The words to find.
    query: String,
    /// "praise", "complaint" or "neutral", to page through one side only.
    side: Option<String>,
    /// A subject's id, to page through the claims filed under it only.
    subject: Option<String>,
    /// How many claims to skip, for the next page.
    from: usize,
    /// How many claims to give.
    count: usize,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ByMeaning {
    /// A prepared game's Steam app id.
    app_id: u32,
    /// The meaning to find claims for.
    query: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ChooseMeaning {
    /// True to prepare every game read from now on.
    every_game: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ChooseWriter {
    /// Whether preparing a game also writes its paragraph.
    write: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BeforeAfter {
    /// The game's Steam app id.
    app_id: u32,
    /// The update's id, as `game_updates` gives it.
    gid: String,
    /// One kind of reviewer only, one of the kinds `reading` lists under `who`, or none for
    /// every reviewer.
    kind: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SaveSettings {
    /// Every setting, as `settings` gives them.
    settings: settings::Settings,
    /// Whether every game read is prepared for search by meaning too.
    search_every_game: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FreeRoom {
    /// The game to remove from, or none to remove that part from every game.
    app_id: Option<u32>,
    /// What to remove.
    what: storage::Removal,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RemoveModel {
    /// The model's key, as `storage` lists it.
    key: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OneGroup {
    /// The group's name, as `groups` gives it.
    name: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SaveGroups {
    /// Every group, as `groups` gives them. A group left out is deleted.
    groups: cockpit::Groups,
}

/// A page of the window.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Page {
    /// What moved, what is running and what needs a look.
    Cockpit,
    /// Every game kept.
    Library,
    /// Finding a game on Steam to add.
    Finder,
    /// How the app runs.
    Settings,
    /// The AI assistants it is added to, and the HTTP server.
    Assistants,
    /// What the library and the models take on disk.
    Storage,
    /// This copy's version and the newest release.
    About,
    /// One game's page. Needs `app_id`.
    Game,
    /// Games side by side. Needs `app_ids`.
    Compare,
    /// What players say across one group of games. Needs `group`.
    Group,
    /// The reviews behind one subject of a read game. Needs `app_id` and `subject`.
    Subject,
    /// A search of a read game's claims. Needs `app_id` and `query`.
    Search,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Show {
    /// Which page of the window to show.
    page: Page,
    /// The game, for a game's page, a subject or a search.
    app_id: Option<u32>,
    /// The games to compare.
    app_ids: Option<Vec<u32>>,
    /// The group's name, as `groups` gives it.
    group: Option<String>,
    /// The subject's id, as `reading` gives it.
    subject: Option<String>,
    /// "praise", "complaint" or "neutral", to open a subject on one side.
    side: Option<String>,
    /// The words to search a game for.
    query: Option<String>,
}

impl Show {
    fn missing(&self) -> Option<&'static str> {
        match self.page {
            Page::Game | Page::Subject | Page::Search if self.app_id.is_none() => Some("app_id"),
            Page::Compare if self.app_ids.as_ref().is_none_or(Vec::is_empty) => Some("app_ids"),
            Page::Group if self.group.is_none() => Some("group"),
            Page::Subject if self.subject.is_none() => Some("subject"),
            Page::Search if self.query.is_none() => Some("query"),
            _ => None,
        }
    }
}

/// The picture's type, from its first bytes: the store serves JPEG, and anything else is said.
fn picture_type(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => "image/webp",
        _ => "image/jpeg",
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one entry per window command, each a few lines; split up, the list stops being one to check"
)]
fn entries() -> Vec<Entry> {
    use Effect::{Changes, Looks, Removes};
    vec![
        tool(
            "library",
            Looks,
            "The games kept, each with how far it has got (downloaded, read), and where the library folder is.",
            |app, Nothing {}| off_thread(move || super::library(app)),
        ),
        tool(
            "games",
            Looks,
            "Every game kept, with its figures, its latest change and whether Steam has new reviews for it.",
            |app, Nothing {}| cockpit::games(app),
        ),
        tool(
            "overview",
            Looks,
            "The cockpit: what changed since the person last looked, what is running, what needs a look and what this machine reads with.",
            |app, Nothing {}| cockpit::overview(app),
        ),
        tool(
            "look_up",
            Looks,
            "What the Steam store says about a game now: its name, how many reviews it has and their verdict.",
            |app, OneGame { app_id }| super::look_up(app, app_id),
        ),
        tool(
            "find_games",
            Looks,
            "Finds games on the Steam store by name, with their app ids.",
            |_, Words { words }| super::find_games(words),
        ),
        answering(
            "art",
            Looks,
            "A game's header picture from the store.",
            |app, OneGame { app_id }| async move {
                let bytes = super::art_bytes(&app, app_id).await?;
                let kind = picture_type(&bytes);
                Ok(vec![ContentBlock::image(
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                    kind,
                )])
            },
        ),
        tool(
            "reading",
            Looks,
            "A read game's figures: every subject, how often players raise, praise and criticise it, and the terms that stand out. Where the game is prepared for search by meaning, also the writer's paragraph and the themes said most widely on each side. Monthly figures, kinds of reviewer and languages come when asked for.",
            |app, asked: ReadingAsked| {
                off_thread_trying(move || {
                    super::reading(app, asked.app_id).and_then(|found| as_asked(&found, &asked))
                })
            },
        ),
        tool(
            "induced",
            Looks,
            "What a read game's players talk about that the fixed subjects do not name, with quoted reviews, where SteamGauge has any.",
            |app, OneGame { app_id }| off_thread_trying(move || super::induced(app, app_id)),
        ),
        tool(
            "who_wrote",
            Looks,
            "One kind of reviewer beside another, or beside everyone else: how each subject's figures differ between them.",
            |app,
             WhoWrote {
                 app_id,
                 these,
                 others,
             }| off_thread_trying(move || who::who_wrote(app, app_id, these, others)),
        ),
        tool(
            "claims_behind",
            Looks,
            "A page of the claims behind one subject of a read game, each with its review. It can narrow to one side, term, kind of reviewer or theme.",
            |app, a: ClaimsBehind| {
                super::claims_behind(
                    app, a.app_id, a.subject, a.side, a.term, a.who, a.theme, a.from, a.count,
                )
            },
        ),
        tool(
            "search_game",
            Looks,
            "Every claim of a read game that says the words searched, counted by side and subject, with a page of them.",
            |app, a: SearchGame| async move {
                let last = app.state::<LastSearch>();
                super::search_game(
                    app.clone(),
                    last,
                    a.app_id,
                    a.query,
                    a.side,
                    a.subject,
                    a.from,
                    a.count,
                )
                .await
            },
        ),
        tool(
            "meaning_offer",
            Looks,
            "Whether a game is ready for search by meaning, and what preparing it would take on this machine.",
            |app, OneGame { app_id }| {
                off_thread_trying(move || {
                    let work = app.state::<work::Work>();
                    super::meaning_offer(app.clone(), work, app_id)
                })
            },
        ),
        tool(
            "choose_meaning",
            Changes,
            "Sets whether every game read from now on is also prepared for search by meaning.",
            |app, ChooseMeaning { every_game }| {
                off_thread_trying(move || super::choose_meaning(app, every_game))
            },
        ),
        tool(
            "writer_offer",
            Looks,
            "Whether a paragraph is written when a game is prepared, and what the writer would take to download and run on this machine.",
            |app, Nothing {}| off_thread(move || super::writer_offer(app)),
        ),
        tool(
            "choose_writer",
            Changes,
            "Sets whether preparing a game for search by meaning also writes a paragraph of what stands out, with a model on this machine.",
            |app, ChooseWriter { write }| {
                off_thread_trying(move || super::choose_writer(app, write))
            },
        ),
        tool(
            "search_by_meaning",
            Looks,
            "The claims of a prepared game nearest in meaning to what was searched, beyond those its words found, best first.",
            |app, a: ByMeaning| async move {
                let models = app.state::<Models>();
                let last = app.state::<LastSearch>();
                super::search_by_meaning(app.clone(), models, last, a.app_id, a.query).await
            },
        ),
        tool(
            "read_offer",
            Looks,
            "What reading a game would take on this machine, for each reader size, and which is recommended.",
            |app, ReadOffer { app_id, language }| {
                off_thread_trying(move || super::read_offer(app, app_id, language))
            },
        ),
        tool(
            "game_updates",
            Looks,
            "The updates a game's developer has posted on Steam, with what moved in the reviews around each.",
            |app, OneGame { app_id }| off_thread(move || game_updates::game_updates(app, app_id)),
        ),
        tool(
            "before_after",
            Looks,
            "Each subject in the four weeks before one of a game's updates against the four weeks after.",
            |app, BeforeAfter { app_id, gid, kind }| {
                game_updates::before_after(app, app_id, gid, kind)
            },
        ),
        tool(
            "since_last_look",
            Looks,
            "What moved in one game since the person last looked at its page.",
            |app, OneGame { app_id }| since::since_last_look(app, app_id),
        ),
        tool(
            "looked",
            Changes,
            "Marks a game's page, or the cockpit when no game is named, as seen now, so only later changes count as new there.",
            |app, MaybeGame { app_id }| since::looked(app, app_id),
        ),
        tool(
            "compare",
            Looks,
            "Games side by side: each subject's figures for each.",
            |app, SomeGames { app_ids }| cockpit::compare(app, app_ids),
        ),
        tool(
            "groups",
            Looks,
            "The groups the person sorts their games into.",
            |app, Nothing {}| off_thread(move || cockpit::groups(app)),
        ),
        tool(
            "group_reading",
            Looks,
            "What players say across one group's read games: each subject's share, praise and complaint, how many games raise it and which most. Shares come with every game counting the same and with every review counting the same, beside each game's own figures.",
            |app, OneGroup { name }| cockpit::group_reading(app, name),
        ),
        tool(
            "save_groups",
            Changes,
            "Replaces the groups the person sorts their games into.",
            |app, SaveGroups { groups }| {
                off_thread_trying(move || cockpit::save_groups(app, groups))
            },
        ),
        tool(
            "work",
            Looks,
            "The jobs on the board: what each is, where it stands, how far it has got and how long is left. Gives every running job and the nearest few of each other state, with how many were left out.",
            |app, asked: WorkAsked| {
                off_thread(move || work_as_asked(work::work(app.state::<work::Work>()), &asked))
            },
        ),
        tool(
            "queue",
            Changes,
            "Starts jobs: downloads, updates, reads, recounts, preparations for search by meaning, checks, model downloads and exports to a given path. Returns their ids at once. `work` shows how they go.",
            |app, Tasks { tasks }| {
                off_thread(move || work::queue(app.clone(), app.state::<work::Work>(), tasks))
            },
        ),
        tool(
            "queue_reads",
            Changes,
            "Queues a read of each game, in the language it was last read in or the one set for first readings.",
            |app, SomeGames { app_ids }| off_thread(move || cockpit::queue_reads(app, app_ids)),
        ),
        tool(
            "queue_updates",
            Changes,
            "Queues an update of these games, or every game: new and edited reviews, then a new read of any game read before.",
            |app, MaybeGames { app_ids }| off_thread(move || cockpit::queue_updates(app, app_ids)),
        ),
        tool(
            "stop_job",
            Changes,
            "Stops a job, or takes it off the board if it has not started.",
            |app, OneJob { id }| {
                off_thread(move || work::stop_job(app.clone(), app.state::<work::Work>(), id))
            },
        ),
        tool(
            "clear_finished",
            Changes,
            "Takes every finished job off the board.",
            |app, Nothing {}| {
                off_thread(move || work::clear_finished(app.clone(), app.state::<work::Work>()))
            },
        ),
        tool(
            "open_report",
            Changes,
            "Opens a finished report job's page in the person's browser.",
            |app, OneJob { id }| {
                off_thread_trying(move || {
                    work::open_report(app.clone(), app.state::<work::Work>(), id)
                })
            },
        ),
        tool(
            "export_data",
            Changes,
            "Asks the person where to save a read game's data, then starts writing it. To write to a known path without asking, use `queue` with a task of kind `export_data`.",
            |app, OneGame { app_id }| data_export::export_data(app, app_id),
        ),
        tool(
            "export_report",
            Changes,
            "Asks the person where to save a report of these games, then starts writing it. To write to a known path without asking, use `queue` with a task of kind `export`.",
            |app, SomeGames { app_ids }| cockpit::export_report(app, app_ids),
        ),
        tool(
            "settings",
            Looks,
            "The app's settings: the share of the graphics card a read may take, reader size, first-reading language, checks and library folder. Also the MCP clients on this computer and how assistants reach the app.",
            |app, Nothing {}| off_thread(move || settings::settings(app)),
        ),
        tool(
            "save_settings",
            Changes,
            "Replaces every setting at once, so give all of them.",
            |app,
             SaveSettings {
                 settings: chosen,
                 search_every_game,
             }| {
                off_thread_trying(move || settings::save_settings(app, chosen, search_every_game))
            },
        ),
        tool(
            "new_http_token",
            Changes,
            "Draws a new token for the HTTP server and returns the settings with it. Clients holding the old token are shut out.",
            |app, Nothing {}| off_thread_trying(move || settings::new_http_token(app)),
        ),
        tool(
            "connect_client",
            Changes,
            "Adds this copy of the app to an MCP client's configuration and returns the settings. The client can reach the open app from then on.",
            |app, OneClient { client }| {
                off_thread_trying(move || settings::connect_client(app, client))
            },
        ),
        tool(
            "disconnect_client",
            Changes,
            "Takes the app out of an MCP client's configuration and returns the settings.",
            |app, OneClient { client }| {
                off_thread_trying(move || settings::disconnect_client(app, client))
            },
        ),
        tool(
            "reader_options",
            Looks,
            "The reader sizes this machine can read with, what each costs to download, and the room left for them.",
            |app, Nothing {}| off_thread(move || settings::reader_options(app)),
        ),
        tool(
            "storage",
            Looks,
            "What the library and the models take on disk, game by game and part by part, and the room left on each drive.",
            |app, Nothing {}| async move { Ok::<_, String>(storage::storage(app).await) },
        ),
        tool(
            "free_room",
            Removes,
            "Removes a part of one game or of every game, or a whole game, to free disk space.",
            |app, FreeRoom { app_id, what }| async move {
                let work = app.state::<work::Work>();
                storage::free_room(app.clone(), work, app_id, what).await
            },
        ),
        tool(
            "remove_model",
            Removes,
            "Removes a downloaded model.",
            |app, RemoveModel { key }| async move {
                let work = app.state::<work::Work>();
                storage::remove_model(app.clone(), work, key).await
            },
        ),
        tool(
            "newer_version",
            Looks,
            "The newer SteamGauge release last heard of, if there is one.",
            |app, Nothing {}| off_thread(move || newer::newer_version(app)),
        ),
        tool(
            "about",
            Looks,
            "This copy's version, whether its build was checked on install, and the version it was updated from. Also the newest release GitHub last named, who makes SteamGauge, what it is built on and where it is published.",
            |app, Nothing {}| off_thread(move || newer::about(app)),
        ),
        tool(
            "whats_new",
            Looks,
            "What each release up to the newest changes, as the About page lists it. With no newer release, it gives the changes since the version this copy was updated from. Asks GitHub's releases API.",
            |app, Nothing {}| newer::whats_new(app),
        ),
        tool(
            "saw_what_changed",
            Changes,
            "Records that the person has seen what changed in the last update, so the sidebar stops pointing at it.",
            |app, Nothing {}| off_thread_trying(move || newer::saw_what_changed(app)),
        ),
        tool(
            "open_notices",
            Looks,
            "Opens the licences and notices of the open-source software SteamGauge is built on, in the system's text viewer.",
            |app, Nothing {}| off_thread_trying(move || newer::open_notices(app)),
        ),
        tool(
            "check_for_updates",
            Changes,
            "Asks GitHub now for the newest SteamGauge release and returns what `about` returns.",
            |app, Nothing {}| newer::check_for_updates(app),
        ),
        tool(
            "update_state",
            Looks,
            "Where an update of SteamGauge stands, or what keeps this copy from updating itself.",
            |app, Nothing {}| {
                off_thread(move || update::update_state(app.state::<update::Updating>()))
            },
        ),
        tool(
            "update_now",
            Changes,
            "Updates SteamGauge to the newest release once its build provenance verifies, then restarts it. The next call reaches the new version.",
            |app, Nothing {}| {
                off_thread(move || update::update_now(app.clone(), app.state::<update::Updating>()))
            },
        ),
        tool(
            "show_update_file",
            Changes,
            "Opens the system's file manager at a verified update the app could not install, so the person can install it by hand.",
            |app, Nothing {}| {
                off_thread_trying(move || update::show_update_file(app.state::<update::Updating>()))
            },
        ),
        tool(
            "show",
            Looks,
            "Brings the window forward on the page asked for.",
            |app, page: Show| async move {
                if let Some(missing) = page.missing() {
                    return Err(format!("that page needs {missing}"));
                }
                bring_forward(&app);
                app.emit("show", &page).map_err(text)
            },
        ),
    ]
}

static TOOLS: LazyLock<Vec<Entry>> = LazyLock::new(entries);

/// One client's server: what it answers is the app's, so every client is served the same app.
#[derive(Clone)]
pub(super) struct Server {
    app: AppHandle,
}

impl Server {
    pub(super) fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

/// Every tool as the documentation describes it.
pub(crate) fn reference() -> Vec<crate::reference::Tool> {
    entries()
        .into_iter()
        .map(|entry| crate::reference::Tool {
            name: entry.tool.name.to_string(),
            description: entry
                .tool
                .description
                .as_deref()
                .unwrap_or_default()
                .to_owned(),
            read_only: entry.effect == Effect::Looks,
            destructive: entry.effect == Effect::Removes,
            input: serde_json::Value::Object((*entry.tool.input_schema).clone()),
        })
        .collect()
}

pub(super) fn server_config() -> ServerConfig {
    ServerConfig::new(
        ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .build(),
    )
    .with_server_info(Implementation::new("steamgauge", env!("CARGO_PKG_VERSION")))
    .with_instructions(INSTRUCTIONS)
}

pub(super) fn tool_list() -> ListToolsResult {
    ListToolsResult::with_all_items(TOOLS.iter().map(|entry| entry.tool.clone()).collect())
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        server_config()
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        std::future::ready(Ok(tool_list()))
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListPromptsResult, ErrorData>> + Send + '_ {
        std::future::ready(Ok(crate::prompts::list()))
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<GetPromptResponse, ErrorData>> + Send + '_ {
        std::future::ready(
            crate::prompts::get(&request.name, request.arguments.as_ref()).map(Into::into),
        )
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let Some(entry) = TOOLS.iter().find(|entry| entry.tool.name == request.name) else {
            return Err(ErrorData::invalid_params(
                format!("SteamGauge has no tool called {}", request.name),
                None,
            ));
        };
        let answer = (entry.call)(self.app.clone(), request.arguments.unwrap_or_default()).await;
        if entry.effect != Effect::Looks {
            // The page on screen draws itself again, so the person sees what was done.
            let _ = self.app.emit("steered", ());
        }
        Ok(match answer {
            Ok(content) => CallToolResult::success(content),
            Err(why) => CallToolResult::error(vec![ContentBlock::text(why)]),
        }
        .into())
    }
}

/// Brings the window forward.
pub fn bring_forward(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Shows the window and starts the server, which lasts as long as the app does.
pub fn start(app: &AppHandle) {
    bring_forward(app);
    let listening = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = listen(&listening).await {
            eprintln!("the MCP server could not start: {error}");
        }
    });
}

pub(super) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut spelled, byte| {
        let _ = write!(spelled, "{byte:02x}");
        spelled
    })
}

/// A pipe only this user can reach. Its name is drawn at random each time the app opens and is
/// told only through the user's own folder, so nobody else can find it or take it first. Pipes
/// refuse other machines, and other users are given read access alone, which sends nothing.
#[cfg(windows)]
fn bind(_local: &Path) -> io::Result<(Listener, String)> {
    let mut drawn = [0_u8; 16];
    getrandom::fill(&mut drawn).map_err(io::Error::other)?;
    let written = format!("steamgauge-mcp-{}", hex(&drawn));
    let listener = ListenerOptions::new()
        .name(socket_name(&written)?)
        .create_tokio()?;
    Ok((listener, written))
}

/// A socket file in a folder only this user may enter.
#[cfg(unix)]
fn bind(local: &Path) -> io::Result<(Listener, String)> {
    use std::os::unix::fs::PermissionsExt;
    let folder = local.join("mcp");
    std::fs::create_dir_all(&folder)?;
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))?;
    let written = folder.join("socket").to_string_lossy().into_owned();
    let listener = ListenerOptions::new()
        .name(socket_name(&written)?)
        .try_overwrite(true)
        .create_tokio()?;
    Ok((listener, written))
}

async fn listen(app: &AppHandle) -> io::Result<()> {
    let local = app.path().app_local_data_dir().map_err(io::Error::other)?;
    std::fs::create_dir_all(&local)?;
    let (listener, written) = bind(&local)?;
    let file = rendezvous(&local);
    let fresh = file.with_extension("new");
    std::fs::write(&fresh, &written)?;
    std::fs::rename(&fresh, &file)?;
    loop {
        let stream = listener.accept().await?;
        tauri::async_runtime::spawn(serve(app.clone(), stream));
    }
}

async fn serve(app: AppHandle, stream: Stream) {
    if let Ok(running) = Server::new(app).serve(stream).await {
        let _ = running.waiting().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The names the window's commands are registered under, read from the registration itself.
    fn window_commands() -> Vec<String> {
        let source = include_str!("mod.rs");
        let list = source
            .split("generate_handler![")
            .nth(1)
            .and_then(|rest| rest.split(']').next())
            .expect("the window registers its commands");
        list.split(',')
            .map(|path| {
                path.trim()
                    .rsplit("::")
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
            .filter(|name| !name.is_empty())
            .collect()
    }

    #[test]
    fn a_reading_gives_ten_themes_a_side_and_the_rest_only_when_asked() {
        let found = serde_json::json!({
            "app_id": 1,
            "subjects": [{ "id": "price" }],
            "themes": {
                "praise": (0..15).collect::<Vec<u32>>(),
                "complaint": (0..3).collect::<Vec<u32>>(),
            },
            "months": [1],
            "who": { "kinds": [] },
            "languages": [1],
        });
        let asked = |themes, months, who, languages| ReadingAsked {
            app_id: 1,
            themes,
            months,
            who,
            languages,
        };

        let brief = as_asked(&found, &asked(None, false, false, false)).unwrap();
        assert_eq!(brief["themes"]["praise"].as_array().unwrap().len(), 10);
        assert_eq!(brief["themes"]["complaint"].as_array().unwrap().len(), 3);
        assert_eq!(brief["subjects"], found["subjects"]);
        for part in ["months", "who", "languages"] {
            assert!(brief.get(part).is_none(), "{part} given unasked");
        }

        let whole = as_asked(&found, &asked(Some(100), true, true, true)).unwrap();
        assert_eq!(whole, found);
        let one = as_asked(&found, &asked(Some(1), false, true, false)).unwrap();
        assert_eq!(one["themes"]["praise"], serde_json::json!([0]));
        assert!(one.get("who").is_some() && one.get("months").is_none());
    }

    #[test]
    fn the_board_gives_every_running_job_and_ten_of_each_other_state_unless_asked() {
        let job = |id: u64, app_id: u32, state: work::State, ended: Option<i64>| work::Job {
            id,
            task: work::Task::Download { app_id },
            name: String::new(),
            state,
            step: String::new(),
            unit: work::Unit::Reviews,
            done: 0.0,
            total: None,
            rate: None,
            left: None,
            note: None,
            queued: 0,
            started: None,
            ended,
            background: false,
        };
        let mut board: Vec<work::Job> = (0..12)
            .map(|id| job(id, 1, work::State::Running, None))
            .collect();
        board.extend((12..42).map(|id| job(id, 2, work::State::Queued, None)));
        board.extend((42..57).map(|id| {
            let ended = i64::try_from(id).unwrap();
            job(id, 3, work::State::Done, Some(ended))
        }));
        board.push(job(57, 3, work::State::Failed, Some(1)));
        let ids = |given: &serde_json::Value| -> Vec<u64> {
            given["jobs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|job| job["id"].as_u64().unwrap())
                .collect()
        };
        let asked = |app_id, jobs| WorkAsked { app_id, jobs };

        let brief = work_as_asked(board.clone(), &asked(None, None));
        let mut wanted: Vec<u64> = (0..22).collect();
        wanted.push(57);
        wanted.extend((47..57).rev());
        assert_eq!(ids(&brief), wanted);
        assert_eq!(
            brief["left_out"],
            serde_json::json!({ "queued": 20, "done": 5 })
        );

        let one_game = work_as_asked(board.clone(), &asked(Some(3), Some(1)));
        assert_eq!(ids(&one_game).len(), 16);
        assert_eq!(one_game["left_out"], serde_json::json!({}));

        let fewer = work_as_asked(board, &asked(None, Some(2)));
        assert_eq!(ids(&fewer).len(), 12 + 2 + 1 + 2);
    }

    #[test]
    fn every_window_command_is_a_tool() {
        let tools: Vec<String> = entries()
            .iter()
            .map(|entry| entry.tool.name.to_string())
            .collect();
        let missing: Vec<String> = window_commands()
            .into_iter()
            .filter(|command| !tools.contains(command))
            .collect();
        assert!(missing.is_empty(), "no tool for {missing:?}");
    }

    #[test]
    fn tool_names_are_unique_and_every_input_is_an_object() {
        let mut seen = std::collections::HashSet::new();
        for entry in entries() {
            assert!(
                seen.insert(entry.tool.name.clone()),
                "{} twice",
                entry.tool.name
            );
            assert_eq!(
                entry.tool.input_schema.get("type"),
                Some(&serde_json::json!("object")),
                "{}",
                entry.tool.name
            );
        }
    }

    #[test]
    fn only_removals_are_marked_destructive_and_only_looks_read_only() {
        for entry in entries() {
            let notes = entry
                .tool
                .annotations
                .expect("every tool says what it does");
            assert_eq!(notes.read_only_hint, Some(entry.effect == Effect::Looks));
            assert_eq!(
                notes.destructive_hint,
                Some(entry.effect == Effect::Removes)
            );
        }
    }

    #[test]
    fn a_page_that_needs_a_game_says_so() {
        let show = |page| Show {
            page,
            app_id: None,
            app_ids: None,
            group: None,
            subject: None,
            side: None,
            query: None,
        };
        assert_eq!(show(Page::Game).missing(), Some("app_id"));
        assert_eq!(show(Page::Compare).missing(), Some("app_ids"));
        assert_eq!(show(Page::Group).missing(), Some("group"));
        assert_eq!(show(Page::Library).missing(), None);
        let subject = Show {
            app_id: Some(10),
            ..show(Page::Subject)
        };
        assert_eq!(subject.missing(), Some("subject"));
    }

    #[test]
    fn a_picture_is_named_by_its_first_bytes() {
        assert_eq!(picture_type(&[0xff, 0xd8, 0xff]), "image/jpeg");
        assert_eq!(picture_type(b"\x89PNG\r\n"), "image/png");
        assert_eq!(picture_type(b"RIFF\0\0\0\0WEBPVP8"), "image/webp");
    }

    #[test]
    fn a_drawn_name_is_spelled_in_hex() {
        assert_eq!(hex(&[0, 15, 255]), "000fff");
    }

    /// A folder of its own for one test. It stands in for the app's local data.
    fn folder() -> std::path::PathBuf {
        let mut drawn = [0_u8; 8];
        getrandom::fill(&mut drawn).unwrap();
        let folder = std::env::temp_dir().join(format!("steamgauge-bridge-{}", hex(&drawn)));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    /// Connects as a client of the given era to what `steamgauge mcp` serves, with no app open,
    /// and lists the tools.
    async fn tools_seen_by(lifecycle: rmcp::service::ClientLifecycleMode) -> Vec<String> {
        let (server_end, client_end) = tokio::io::duplex(1 << 16);
        let local = folder();
        let server = tokio::spawn(async move {
            let running = crate::mcp::Bridge::new(local)
                .serve(server_end)
                .await
                .expect("the server starts");
            let _ = running.waiting().await;
        });
        let client = rmcp::service::serve_client_with_lifecycle((), client_end, lifecycle)
            .await
            .expect("the handshake succeeds");
        let tools = client.list_all_tools().await.expect("the tools are listed");
        client.cancel().await.expect("the client closes");
        server.await.expect("the server ends");
        tools
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect()
    }

    fn every_tool() -> Vec<String> {
        entries()
            .iter()
            .map(|entry| entry.tool.name.to_string())
            .collect()
    }

    #[tokio::test]
    async fn a_client_of_the_discover_protocol_sees_every_tool() {
        let seen = tools_seen_by(rmcp::service::ClientLifecycleMode::Discover {
            preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
        })
        .await;
        assert_eq!(seen, every_tool());
    }

    #[tokio::test]
    async fn a_client_of_the_earlier_handshake_sees_every_tool() {
        let seen = tools_seen_by(rmcp::service::ClientLifecycleMode::Initialize).await;
        assert_eq!(seen, every_tool());
    }

    /// An open app as the bridge meets it: a server on the socket the rendezvous names, which
    /// answers every call with the tool's name.
    struct Open;

    impl ServerHandler for Open {
        fn get_info(&self) -> ServerConfig {
            server_config()
        }

        fn call_tool(
            &self,
            request: CallToolRequestParams,
            _context: RequestContext<RoleServer>,
        ) -> impl Future<Output = Result<CallToolResponse, ErrorData>> + Send + '_ {
            std::future::ready(Ok(CallToolResult::success(vec![ContentBlock::text(
                request.name.to_string(),
            )])
            .into()))
        }
    }

    /// Opens a stand-in app on `local` that serves one connection, and returns what ends it.
    fn open_app(local: &Path) -> tokio::task::JoinHandle<()> {
        let (listener, written) = bind(local).unwrap();
        std::fs::write(rendezvous(local), written).unwrap();
        tokio::spawn(async move {
            let stream = listener.accept().await.unwrap();
            let running = Open.serve(stream).await.unwrap();
            let _ = running.waiting().await;
        })
    }

    fn asking(name: &'static str) -> CallToolRequestParams {
        CallToolRequestParams::new(name)
    }

    fn said(answer: CallToolResponse) -> (bool, String) {
        let CallToolResponse::Complete(result) = answer else {
            panic!("the call asked for more input");
        };
        let text = result
            .content
            .iter()
            .filter_map(|block| block.as_text().map(|text| text.text.clone()))
            .collect();
        (result.is_error.unwrap_or(false), text)
    }

    #[tokio::test]
    async fn a_call_while_the_app_is_closed_says_to_ask_the_person() {
        let bridge = crate::mcp::Bridge::new(folder());
        let (failed, text) = said(bridge.call(asking("library")).await.unwrap());
        assert!(failed);
        assert_eq!(text, crate::mcp::CLOSED);
    }

    #[tokio::test]
    async fn a_call_reaches_the_open_app_and_a_closed_one_is_said() {
        let local = folder();
        let app = open_app(&local);
        let bridge = crate::mcp::Bridge::new(local.clone());
        assert_eq!(
            said(bridge.call(asking("library")).await.unwrap()),
            (false, "library".to_owned())
        );
        app.abort();
        let _ = app.await;
        let (failed, text) = said(bridge.call(asking("library")).await.unwrap());
        assert!(failed);
        assert!(
            text == crate::mcp::CLOSED || text == crate::mcp::CLOSED_DURING,
            "{text}"
        );
        let (_, text) = said(bridge.call(asking("library")).await.unwrap());
        assert_eq!(text, crate::mcp::CLOSED);
    }

    /// What a client of the given era receives for a call made through `steamgauge mcp` to an
    /// open app.
    async fn call_seen_by(lifecycle: rmcp::service::ClientLifecycleMode) -> CallToolResult {
        let local = folder();
        let _app = open_app(&local);
        let (server_end, client_end) = tokio::io::duplex(1 << 16);
        let server = tokio::spawn(async move {
            let running = crate::mcp::Bridge::new(local)
                .serve(server_end)
                .await
                .expect("the server starts");
            let _ = running.waiting().await;
        });
        let client = rmcp::service::serve_client_with_lifecycle((), client_end, lifecycle)
            .await
            .expect("the handshake succeeds");
        let answer = client
            .call_tool_once(asking("library"))
            .await
            .expect("the call is answered");
        client.cancel().await.expect("the client closes");
        server.await.expect("the server ends");
        let CallToolResponse::Complete(result) = answer else {
            panic!("the call asked for more input");
        };
        result
    }

    #[tokio::test]
    async fn a_client_of_the_discover_protocol_is_told_a_call_through_the_bridge_is_complete() {
        let result = call_seen_by(rmcp::service::ClientLifecycleMode::Discover {
            preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
        })
        .await;
        assert_eq!(result.result_type, Some(rmcp::model::ResultType::COMPLETE));
        assert_eq!(said(result.into()), (false, "library".to_owned()));
    }

    #[tokio::test]
    async fn a_client_of_the_earlier_handshake_gets_a_call_through_the_bridge_in_its_shape() {
        let result = call_seen_by(rmcp::service::ClientLifecycleMode::Initialize).await;
        assert_eq!(result.result_type, None);
        assert_eq!(said(result.into()), (false, "library".to_owned()));
    }

    #[tokio::test]
    async fn an_app_opened_again_is_reached_again() {
        let local = folder();
        let first = open_app(&local);
        let bridge = crate::mcp::Bridge::new(local.clone());
        assert!(!said(bridge.call(asking("library")).await.unwrap()).0);
        first.abort();
        let _ = first.await;
        let _ = bridge.call(asking("library")).await;
        let _second = open_app(&local);
        assert_eq!(
            said(bridge.call(asking("work")).await.unwrap()),
            (false, "work".to_owned())
        );
    }
}
