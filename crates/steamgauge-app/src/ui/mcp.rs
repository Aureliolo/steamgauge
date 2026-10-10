//! The app as a Model Context Protocol server: every tool is something the window can do, called
//! through the same function the window's command is, so a client and a person can never be
//! offered different things. A client reaches it through `steamgauge mcp` (see `crate::mcp`).
//!
//! While a client is connected the app stays open: closing the window hides it, and an app opened
//! for a client alone closes once the last one has gone and the work it was asked for is done.

use std::{
    future::Future,
    io,
    path::Path,
    pin::Pin,
    sync::{
        LazyLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

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
        CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
        JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig,
        Tool, ToolAnnotations,
    },
    service::RequestContext,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tauri::{AppHandle, Emitter, Manager};

use super::{
    LastSearch, Meaning, cockpit, data_export, game_updates, newer, settings, since, storage, text,
    update, who, work,
};
use crate::mcp::{rendezvous, socket_name};

/// What a client is told the server is for, before it has looked at a single tool.
const INSTRUCTIONS: &str = "SteamGauge downloads every Steam review of a game and reads each \
claim in them with a model on this machine, then shows what players praise and criticise, with \
the reviews behind every figure. Each tool here is something the SteamGauge window can do, and \
the person sees its effects there. A game is named by its Steam app id: find_games looks one up \
by name. Downloads, reads, updates and exports are jobs: queue starts them and returns at once, \
work shows where each stands. A game is read once its download is done; reading then gives its \
subjects and figures, and claims_behind and search_game the reviews behind them. show puts a \
page in front of the person.";

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
struct OneGame {
    /// The game's Steam app id, as in its store address.
    app_id: u32,
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
    /// The jobs to start, run in the order given within each lane: downloads and checks share
    /// the network, reads and preparations the graphics card, exports the disk.
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
    /// The game's Steam app id, which has to be prepared for search by meaning.
    app_id: u32,
    /// What to find claims meaning.
    query: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ChooseMeaning {
    /// Whether every game read from now on is prepared for search by meaning too.
    every_game: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct BeforeAfter {
    /// The game's Steam app id.
    app_id: u32,
    /// The update's id, as `game_updates` gives it.
    gid: String,
    /// One kind of reviewer only, or none for every reviewer.
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
struct SaveGroups {
    /// Every group, as `groups` gives them; a group left out is deleted.
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
    Settings,
    /// What the library and the models take on disk.
    Storage,
    /// One game's page; needs `app_id`.
    Game,
    /// Games side by side; needs `app_ids`.
    Compare,
    /// The reviews behind one subject of a read game; needs `app_id` and `subject`.
    Subject,
    /// A read game's claims that say something; needs `app_id` and `query`.
    Search,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Show {
    page: Page,
    /// The game, for a game's page, a subject or a search.
    app_id: Option<u32>,
    /// The games to compare.
    app_ids: Option<Vec<u32>>,
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
            "Every game kept, with its figures, its newest movement and whether Steam has new reviews for it: the library page's rows.",
            |app, Nothing {}| cockpit::games(app),
        ),
        tool(
            "overview",
            Looks,
            "The cockpit: what moved since last looked, what is running, what needs a look, and what this machine reads with.",
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
            "A read game's figures: every subject its players talk about, how often, praised and criticised, with the terms that stand out and how the figures move month by month.",
            |app, OneGame { app_id }| off_thread_trying(move || super::reading(app, app_id)),
        ),
        tool(
            "induced",
            Looks,
            "What a read game's players talk about that the fixed subjects do not name, with quoted reviews, where that has been worked out.",
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
            "A page of the claims behind one subject of a read game, with the review each comes from, narrowed to one side, a term or a kind of reviewer if asked.",
            |app, a: ClaimsBehind| {
                super::claims_behind(
                    app, a.app_id, a.subject, a.side, a.term, a.who, a.from, a.count,
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
            "Whether every game read from now on is also prepared for search by meaning.",
            |app, ChooseMeaning { every_game }| {
                off_thread_trying(move || super::choose_meaning(app, every_game))
            },
        ),
        tool(
            "search_by_meaning",
            Looks,
            "The claims of a prepared game nearest in meaning to what was searched, beyond those its words found, best first.",
            |app, a: ByMeaning| async move {
                let meaning = app.state::<Meaning>();
                let last = app.state::<LastSearch>();
                super::search_by_meaning(app.clone(), meaning, last, a.app_id, a.query).await
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
            "Marks a game's page, or the cockpit where no game is named, as seen by the person now, which resets what counts as new there.",
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
            "Every job on the board: what it is, where it stands, how far it has got and how long is left.",
            |app, Nothing {}| off_thread(move || work::work(app.state::<work::Work>())),
        ),
        tool(
            "queue",
            Changes,
            "Starts jobs: downloads, updates, reads, recounts, preparations for search by meaning, checks, and exports written to a path given here. Returns their ids at once; work follows them.",
            |app, Tasks { tasks }| {
                off_thread(move || work::queue(app.clone(), app.state::<work::Work>(), tasks))
            },
        ),
        tool(
            "queue_reads",
            Changes,
            "Reads these games, each in the language it was last read in, or in the one chosen for first readings.",
            |app, SomeGames { app_ids }| off_thread(move || cockpit::queue_reads(app, app_ids)),
        ),
        tool(
            "queue_updates",
            Changes,
            "Updates these games, or every game: what was written or edited since, then a read where one was read.",
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
            "Asks the person, in the system's save dialog, where to save a read game's data, then starts writing it. queue with an export_data task writes to a path without asking.",
            |app, OneGame { app_id }| data_export::export_data(app, app_id),
        ),
        tool(
            "export_report",
            Changes,
            "Asks the person, in the system's save dialog, where to save a report of these games, then starts writing it. queue with an export task writes to a path without asking.",
            |app, SomeGames { app_ids }| cockpit::export_report(app, app_ids),
        ),
        tool(
            "settings",
            Looks,
            "How the app runs: the share of the graphics card a read may take, the reader size, the first-reading language, the checks it makes, and where the library is.",
            |app, Nothing {}| off_thread(move || settings::settings(app)),
        ),
        tool(
            "save_settings",
            Changes,
            "Replaces every setting; give all of them, as settings returns them.",
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
            "Draws a new token for the HTTP server, shutting out every client that holds the old one, and returns the settings with it.",
            |app, Nothing {}| off_thread_trying(move || settings::new_http_token(app)),
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
            "Removes part of a game, or of every game: its readings, its preparation for search by meaning, its earlier downloads, what work stopped part way left, or the whole game.",
            |app, FreeRoom { app_id, what }| async move {
                let work = app.state::<work::Work>();
                storage::free_room(app.clone(), work, app_id, what).await
            },
        ),
        tool(
            "remove_model",
            Removes,
            "Removes a downloaded model; a read that needs it downloads it again.",
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
            "Updates SteamGauge to the newest release once its build provenance verifies, then restarts it, which ends this connection.",
            |app, Nothing {}| {
                off_thread(move || update::update_now(app.clone(), app.state::<update::Updating>()))
            },
        ),
        tool(
            "show_update_file",
            Changes,
            "Shows a verified update the app could not install in the system's file manager, for installing it by hand.",
            |app, Nothing {}| {
                off_thread_trying(move || update::show_update_file(app.state::<update::Updating>()))
            },
        ),
        tool(
            "show",
            Looks,
            "Puts a page of the window in front of the person, opening the window if it is hidden.",
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

/// The command that adds this copy to Claude Code, naming it by its own path, which no
/// installer puts on the search path everywhere.
pub fn claude_command() -> String {
    let program = std::env::current_exe().map_or_else(
        |_| "steamgauge".to_owned(),
        |path| path.display().to_string(),
    );
    format!("claude mcp add steamgauge -- \"{program}\" mcp")
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("steamgauge", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(
            TOOLS.iter().map(|entry| entry.tool.clone()).collect(),
        )))
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

/// How many clients are connected, and whether one ever was.
#[derive(Debug, Default)]
pub struct Clients {
    open: AtomicUsize,
    ever: AtomicBool,
}

impl Clients {
    fn any(&self) -> bool {
        self.open.load(Ordering::SeqCst) > 0
    }
}

/// Brings the window forward, opening it where it was hidden.
pub fn bring_forward(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_dock_visibility(true);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn hide(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_dock_visibility(false);
}

/// Closing the window while a client is connected hides it instead, so the client keeps the app
/// it was steering.
pub fn on_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        let app = window.app_handle();
        if app.state::<Clients>().any() {
            api.prevent_close();
            hide(app);
        }
    }
}

/// How long an app opened for a client waits for it to connect before closing unasked.
const FIRST_CLIENT_WAIT: Duration = Duration::from_secs(60);
const LOOK_EVERY: Duration = Duration::from_secs(5);

/// Starts the server, and where the app was opened for a client, keeps the window hidden and
/// closes the app once no client is connected, the window is still hidden and no job is left.
pub fn start(app: &AppHandle, without_window: bool) {
    if without_window {
        hide(app);
    } else {
        bring_forward(app);
    }
    let listening = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = listen(&listening).await {
            eprintln!("the MCP server could not start: {error}");
        }
    });
    let watched = app.clone();
    let opened = Instant::now();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(LOOK_EVERY).await;
            if needed(&watched, opened) {
                continue;
            }
            watched.exit(0);
            return;
        }
    });
}

fn needed(app: &AppHandle, opened: Instant) -> bool {
    let shown = app
        .get_webview_window("main")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(true);
    let clients = app.state::<Clients>();
    let waiting = !clients.ever.load(Ordering::SeqCst) && opened.elapsed() < FIRST_CLIENT_WAIT;
    let working = app
        .state::<work::Work>()
        .jobs()
        .iter()
        .any(|job| matches!(job.state, work::State::Queued | work::State::Running));
    shown || clients.any() || waiting || working
}

pub(super) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut spelled, byte| {
        let _ = write!(spelled, "{byte:02x}");
        spelled
    })
}

/// A pipe only this user can reach. Its name is drawn at random each time the app opens and is
/// told only through the user's own folder, so nobody else can find it or take it first; pipes
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
    let clients = app.state::<Clients>();
    clients.open.fetch_add(1, Ordering::SeqCst);
    clients.ever.store(true, Ordering::SeqCst);
    if let Ok(running) = Server::new(app.clone()).serve(stream).await {
        let _ = running.waiting().await;
    }
    clients.open.fetch_sub(1, Ordering::SeqCst);
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
            subject: None,
            side: None,
            query: None,
        };
        assert_eq!(show(Page::Game).missing(), Some("app_id"));
        assert_eq!(show(Page::Compare).missing(), Some("app_ids"));
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
}
