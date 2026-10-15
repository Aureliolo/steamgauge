//! The prompts the MCP server offers: requests a person picks from the assistant's own list, such
//! as the commands it shows under `/`, each written out in the tools' words, so an assistant new
//! to `SteamGauge` goes about the request the way the window would. They are text alone, the same
//! from `steamgauge mcp` and from the open app, and need nothing of either.

use rmcp::{
    ErrorData,
    model::{
        GetPromptResult, JsonObject, ListPromptsResult, Prompt, PromptArgument, PromptMessage, Role,
    },
};

/// One argument a prompt is given.
#[derive(Debug)]
pub struct Argument {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
}

/// Each argument's value by name, where it was given.
type Given<'a> = dyn Fn(&str) -> Option<String> + 'a;

/// One prompt: what the assistant lists, and the request it sends once the arguments are in.
#[derive(Debug)]
pub struct Spec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub arguments: &'static [Argument],
    /// The request; a required argument is always given.
    text: fn(&Given) -> String,
}

const GAME: Argument = Argument {
    name: "game",
    description: "The game: its name, its Steam store link or its app id.",
    required: true,
};

/// How a game named any way becomes one that has been read, the same for every prompt.
const READY: &str = "Find the Steam app id of each game: from a store link or a number given, or \
with find_games by its name, asking me which where more than one matches. Check library for each. \
One that is not there needs a download task in queue; one that has not been read, or whose reading \
is not current, needs a read task once its download is done, unless work already shows one. Follow \
work until they are done, telling me how long is left.";

pub const PROMPTS: &[Spec] = &[
    Spec {
        name: "what_players_say",
        title: "What players say about a game",
        description: "What a game's players praise and criticise, from every one of its Steam \
                      reviews, downloading and reading it first where it has to be.",
        arguments: &[GAME],
        text: |given| {
            let game = given("game").unwrap_or_default();
            format!(
                "Use SteamGauge to find out what players of {game} praise and criticise.\n\n\
                 {READY}\n\n\
                 Then, from reading, tell me which subjects players raise most, which they praise \
                 and which they criticise, with the figures. For the largest praise and the \
                 largest complaint, quote two or three reviews from claims_behind. Open the \
                 game's page for me with show."
            )
        },
    },
    Spec {
        name: "compare_games",
        title: "Compare games",
        description: "Where the players of two or more games differ, subject by subject, \
                      downloading and reading each first where it has to be.",
        arguments: &[Argument {
            name: "games",
            description: "Two or more games, by name, Steam store link or app id, separated by \
                          commas.",
            required: true,
        }],
        text: |given| {
            let games = given("games").unwrap_or_default();
            format!(
                "Use SteamGauge to compare what players say about these games: {games}.\n\n\
                 {READY}\n\n\
                 Then set them side by side with compare and tell me the subjects where they \
                 differ most, in praise and in complaints, with the figures, quoting a review \
                 from claims_behind for each difference. Open the comparison for me with show."
            )
        },
    },
    Spec {
        name: "around_an_update",
        title: "How players took an update",
        description: "What moved in a game's reviews in the four weeks after one of its \
                      developer's updates against the four weeks before.",
        arguments: &[
            GAME,
            Argument {
                name: "update",
                description: "The update, by its title or date as Steam lists it; the newest \
                              where none is given.",
                required: false,
            },
        ],
        text: |given| {
            let game = given("game").unwrap_or_default();
            let update = given("update")
                .filter(|update| !update.trim().is_empty())
                .map_or_else(
                    || "its newest update".to_owned(),
                    |update| format!("the update {update}"),
                );
            format!(
                "Use SteamGauge to find out how players of {game} took {update}.\n\n\
                 {READY} A game that was already in the library needs queue_updates as well, \
                 so the reviews written since it was downloaded are in; follow work until that \
                 is done too.\n\n\
                 Then find the update in game_updates and set the four weeks after it against \
                 the four weeks before with before_after. Tell me which subjects moved, which \
                 way and by how much, as before_after gives it, quoting reviews written after the \
                 update from claims_behind."
            )
        },
    },
];

/// Every prompt, as a client lists them.
#[must_use]
pub fn list() -> ListPromptsResult {
    ListPromptsResult::with_all_items(
        PROMPTS
            .iter()
            .map(|spec| {
                Prompt::new(
                    spec.name,
                    Some(spec.description),
                    Some(
                        spec.arguments
                            .iter()
                            .map(|argument| {
                                PromptArgument::new(argument.name)
                                    .with_description(argument.description)
                                    .with_required(argument.required)
                            })
                            .collect(),
                    ),
                )
                .with_title(spec.title)
            })
            .collect(),
    )
}

/// The request a prompt sends with these arguments.
///
/// # Errors
///
/// Fails on a prompt the server does not have, on an argument it does not take, and on a
/// required argument missing, empty or not text.
pub fn get(name: &str, arguments: Option<&JsonObject>) -> Result<GetPromptResult, ErrorData> {
    let spec = PROMPTS
        .iter()
        .find(|spec| spec.name == name)
        .ok_or_else(|| {
            ErrorData::invalid_params(format!("SteamGauge has no prompt called {name}"), None)
        })?;
    let empty = JsonObject::new();
    let arguments = arguments.unwrap_or(&empty);
    if let Some(unknown) = arguments
        .keys()
        .find(|key| !spec.arguments.iter().any(|argument| argument.name == *key))
    {
        return Err(ErrorData::invalid_params(
            format!("{name} takes no argument called {unknown}"),
            None,
        ));
    }
    let text = |key: &str| {
        arguments
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    if let Some(missing) = spec
        .arguments
        .iter()
        .find(|argument| argument.required && text(argument.name).is_none())
    {
        return Err(ErrorData::invalid_params(
            format!("{name} needs {}: {}", missing.name, missing.description),
            None,
        ));
    }
    Ok(GetPromptResult::new(vec![PromptMessage::new_text(
        Role::User,
        (spec.text)(&text),
    )])
    .with_description(spec.description))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn given(pairs: &[(&str, &str)]) -> JsonObject {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), serde_json::Value::from(*value)))
            .collect()
    }

    fn said(result: &GetPromptResult) -> String {
        serde_json::to_value(&result.messages[0].content).unwrap()["text"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn each_prompt_is_listed_with_its_arguments() {
        let listed = list();
        assert_eq!(listed.prompts.len(), PROMPTS.len());
        for (prompt, spec) in listed.prompts.iter().zip(PROMPTS) {
            assert_eq!(prompt.name, spec.name);
            assert_eq!(prompt.title.as_deref(), Some(spec.title));
            let arguments = prompt.arguments.as_ref().unwrap();
            assert_eq!(arguments.len(), spec.arguments.len());
            assert!(arguments.iter().all(|argument| argument.required.is_some()));
        }
    }

    #[test]
    fn a_prompt_says_the_game_it_was_given_and_names_only_tools_the_server_has() {
        let tools: Vec<String> = crate::ui::tool_reference()
            .into_iter()
            .map(|tool| tool.name)
            .collect();
        for spec in PROMPTS {
            let arguments: Vec<(&str, &str)> = spec
                .arguments
                .iter()
                .map(|argument| (argument.name, "Hades II"))
                .collect();
            let text = said(&get(spec.name, Some(&given(&arguments))).unwrap());
            assert!(text.contains("Hades II"), "{text}");
            for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
                if word.contains('_') {
                    assert!(
                        tools.iter().any(|tool| tool == word),
                        "{}: {word}",
                        spec.name
                    );
                }
            }
            for tool in ["find_games", "library", "queue", "work"] {
                assert!(text.contains(tool), "{}: {tool}", spec.name);
            }
        }
    }

    #[test]
    fn an_update_not_named_is_the_newest() {
        let newest = said(&get("around_an_update", Some(&given(&[("game", "Hades II")]))).unwrap());
        assert!(newest.contains("took its newest update"), "{newest}");
        let named = said(
            &get(
                "around_an_update",
                Some(&given(&[("game", "Hades II"), ("update", "Patch 2")])),
            )
            .unwrap(),
        );
        assert!(named.contains("took the update Patch 2"), "{named}");
    }

    #[test]
    fn a_missing_or_unknown_argument_and_an_unknown_prompt_are_refused() {
        assert!(get("what_players_say", None).is_err());
        assert!(get("what_players_say", Some(&given(&[("game", "  ")]))).is_err());
        assert!(
            get(
                "what_players_say",
                Some(&given(&[("game", "Hades II"), ("gmae", "x")]))
            )
            .is_err()
        );
        assert!(get("nothing", Some(&given(&[("game", "Hades II")]))).is_err());
    }
}
