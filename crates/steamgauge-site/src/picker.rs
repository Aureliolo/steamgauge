//! A row of chips, each choosing one panel: radio inputs ahead of their chips and panels, so
//! choosing needs no script, which the site's content security policy forbids. The rules tying
//! each input to its own chip and panel are written with them, in a `<style-rules>` element the
//! build moves into the stylesheet, since a page carries no inline style.

use std::fmt::Write as _;

use crate::escape;

#[derive(Debug)]
pub struct Choice {
    pub id: String,
    pub name: String,
    /// What to type, shown as it is.
    pub code: String,
    /// What it does, in the little Markdown `inline` reads.
    pub note: String,
}

/// The picker for `choices`, the first chosen to start with. `group` keeps its ids apart from
/// another picker's on the same page.
#[must_use]
pub fn html(group: &str, choices: &[Choice]) -> String {
    let mut inputs = String::new();
    let mut chips = String::new();
    let mut panels = String::new();
    let mut rules = String::new();
    for (at, choice) in choices.iter().enumerate() {
        let id = format!("{group}-{}", choice.id);
        let checked = if at == 0 { " checked" } else { "" };
        let _ = writeln!(
            inputs,
            "<input type=\"radio\" name=\"{group}\" id=\"pick-{id}\" aria-controls=\"panel-{id}\"{checked}>"
        );
        let _ = write!(
            chips,
            "<label for=\"pick-{id}\">{}</label>",
            escape(&choice.name)
        );
        let _ = write!(
            panels,
            "<div class=\"panel\" id=\"panel-{id}\"><pre tabindex=\"0\"><code>{}</code></pre>\
             <p>{}</p></div>",
            commented(&choice.code),
            inline(&choice.note)
        );
        let _ = write!(
            rules,
            "#pick-{id}:checked~.panels #panel-{id}{{display:block}}\
             #pick-{id}:checked~.chips label[for=\"pick-{id}\"]{{background:var(--accent);\
             border-color:var(--accent);color:var(--accent-ink)}}\
             #pick-{id}:focus-visible~.chips label[for=\"pick-{id}\"]{{outline:3px solid \
             var(--accent);outline-offset:2px}}"
        );
    }
    format!(
        "<div class=\"picker\">\n{inputs}<div class=\"chips\">{chips}</div>\n\
         <div class=\"panels\">{panels}</div>\n</div>\n<style-rules>{rules}</style-rules>"
    )
}

/// `page` without its pickers' rules, and the rules, for the stylesheet.
#[must_use]
pub fn rules(page: &str) -> (String, String) {
    let mut kept = String::with_capacity(page.len());
    let mut rules = String::new();
    let mut rest = page;
    while let Some(at) = rest.find("<style-rules>") {
        kept.push_str(&rest[..at]);
        let after = &rest[at + "<style-rules>".len()..];
        let (these, next) = after.split_once("</style-rules>").unwrap_or((after, ""));
        rules.push_str(these);
        rest = next;
    }
    kept.push_str(rest);
    (kept, rules)
}

/// Code with its shell comments marked, so the stylesheet can set them apart.
#[must_use]
pub fn commented(code: &str) -> String {
    code.lines()
        .map(|line| {
            if line.starts_with('#') {
                format!("<span class=\"comment\">{}</span>", escape(line))
            } else {
                escape(line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The little Markdown the cards and notes use: `code` and **strong**.
#[must_use]
pub fn inline(text: &str) -> String {
    let mut out = String::new();
    let mut code = false;
    let mut strong = false;
    let escaped = escape(text);
    let mut rest = escaped.as_str();
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("**") {
            out.push_str(if strong { "</strong>" } else { "<strong>" });
            strong = !strong;
            rest = after;
        } else if let Some(after) = rest.strip_prefix('`') {
            out.push_str(if code { "</code>" } else { "<code>" });
            code = !code;
            rest = after;
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn choices() -> Vec<Choice> {
        ["one", "two"]
            .iter()
            .map(|id| Choice {
                id: (*id).to_owned(),
                name: id.to_uppercase(),
                code: format!("# {id}\nrun {id}"),
                note: format!("Does `{id}`."),
            })
            .collect()
    }

    #[test]
    fn every_choice_has_a_chip_a_panel_and_its_rules_and_the_first_is_chosen() {
        let (page, rules) = rules(&html("way", &choices()));
        for id in ["one", "two"] {
            assert!(page.contains(&format!("id=\"pick-way-{id}\"")), "{id}");
            assert!(
                page.contains(&format!("<label for=\"pick-way-{id}\">")),
                "{id}"
            );
            assert!(page.contains(&format!("id=\"panel-way-{id}\"")), "{id}");
            assert!(rules.contains(&format!(
                "#pick-way-{id}:checked~.panels #panel-way-{id}{{display:block}}"
            )));
        }
        assert_eq!(page.matches(" checked>").count(), 1);
        assert!(page.contains("id=\"pick-way-one\" aria-controls=\"panel-way-one\" checked>"));
        assert!(!page.contains("style"), "{page}");
        assert!(page.contains("<span class=\"comment\"># one</span>\nrun one"));
    }

    #[test]
    fn two_pickers_on_a_page_give_up_both_their_rules() {
        let page = format!("a{}b{}c", html("x", &choices()), html("y", &choices()));
        let (kept, rules) = rules(&page);
        assert!(rules.contains("#pick-x-one") && rules.contains("#pick-y-two"));
        assert!(!kept.contains("style-rules"));
    }

    #[test]
    fn the_little_markdown_becomes_code_and_strong() {
        assert_eq!(
            inline("**Add** `a` <b>"),
            "<strong>Add</strong> <code>a</code> &lt;b&gt;"
        );
    }
}
