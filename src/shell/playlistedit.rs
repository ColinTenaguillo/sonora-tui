//! Editing playlists: a track into one (`P`), a new one (`N`), a rename
//! (`m`), a deletion (`D`). Each asks something first -- which playlist,
//! what name, are you sure -- and the question is a small modal over the
//! view that takes the keys while it is up. Taking a track out (`x`) asks
//! nothing and lives with the other keys.
//!
//! Only the account's own playlists are offered or touched: the service
//! says which those are, and a followed playlist or a mix is read-only.

use crossterm::event::KeyCode;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::domain::TrackId;
use crate::library::Playlist;

use super::theme::Palette;

/// The question up, and what has been typed or picked so far.
#[derive(Debug, Clone)]
pub enum Prompt {
    /// Which playlist takes the track. Row 0 is a new one, named in place
    /// once picked; the rest are the account's own, in library order.
    AddTo {
        track: TrackId,
        title: String,
        selected: usize,
        naming: Option<String>,
    },
    NewPlaylist {
        name: String,
    },
    Rename {
        uuid: String,
        name: String,
    },
    Delete {
        uuid: String,
        title: String,
    },
}

/// What the user decided, for the loop to send to the service.
#[derive(Debug, Clone)]
pub enum Edit {
    AddTo {
        uuid: String,
        title: String,
        track: TrackId,
    },
    Create {
        name: String,
        then_add: Option<TrackId>,
    },
    Rename {
        uuid: String,
        name: String,
    },
    Delete {
        uuid: String,
    },
}

/// What a key did to the prompt.
#[derive(Debug)]
pub enum Outcome {
    Kept,
    Closed,
    Done(Edit),
}

/// One key into the prompt. `owned` is the account's own playlists, the
/// ones `AddTo` lists.
pub fn on_key(prompt: &mut Prompt, key: KeyCode, owned: &[Playlist]) -> Outcome {
    match prompt {
        Prompt::AddTo {
            track,
            selected,
            naming: naming @ None,
            ..
        } => match key {
            KeyCode::Char('j') | KeyCode::Down => {
                *selected = (*selected + 1).min(owned.len());
                Outcome::Kept
            }
            KeyCode::Char('k') | KeyCode::Up => {
                *selected = selected.saturating_sub(1);
                Outcome::Kept
            }
            KeyCode::Enter => match *selected {
                0 => {
                    *naming = Some(String::new());
                    Outcome::Kept
                }
                n => match owned.get(n - 1) {
                    Some(p) => Outcome::Done(Edit::AddTo {
                        uuid: p.uuid.clone(),
                        title: p.title.clone(),
                        track: track.clone(),
                    }),
                    None => Outcome::Closed,
                },
            },
            KeyCode::Esc => Outcome::Closed,
            _ => Outcome::Kept,
        },
        Prompt::AddTo {
            track,
            naming: naming @ Some(_),
            ..
        } => {
            let name = naming.as_mut().expect("matched Some");
            match typed(name, key) {
                Typed::Sent => Outcome::Done(Edit::Create {
                    name: name.trim().to_owned(),
                    then_add: Some(track.clone()),
                }),
                // Back to the list rather than out: the name was a
                // second step, so Esc undoes one.
                Typed::Left => {
                    *naming = None;
                    Outcome::Kept
                }
                Typed::Kept => Outcome::Kept,
            }
        }
        Prompt::NewPlaylist { name } => match typed(name, key) {
            Typed::Sent => Outcome::Done(Edit::Create {
                name: name.trim().to_owned(),
                then_add: None,
            }),
            Typed::Left => Outcome::Closed,
            Typed::Kept => Outcome::Kept,
        },
        Prompt::Rename { uuid, name } => match typed(name, key) {
            Typed::Sent => Outcome::Done(Edit::Rename {
                uuid: uuid.clone(),
                name: name.trim().to_owned(),
            }),
            Typed::Left => Outcome::Closed,
            Typed::Kept => Outcome::Kept,
        },
        // Anything but a plain yes is a no: a deletion is the one edit
        // the service cannot take back.
        Prompt::Delete { uuid, .. } => match key {
            KeyCode::Char('y') => Outcome::Done(Edit::Delete { uuid: uuid.clone() }),
            _ => Outcome::Closed,
        },
    }
}

enum Typed {
    Kept,
    /// Enter on a non-blank name.
    Sent,
    /// Esc.
    Left,
}

fn typed(text: &mut String, key: KeyCode) -> Typed {
    match key {
        KeyCode::Char(c) => {
            text.push(c);
            Typed::Kept
        }
        KeyCode::Backspace => {
            text.pop();
            Typed::Kept
        }
        KeyCode::Enter if !text.trim().is_empty() => Typed::Sent,
        KeyCode::Esc => Typed::Left,
        _ => Typed::Kept,
    }
}

fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

/// Rows of the pick list shown at once, besides the "new" row.
const LISTED: usize = 10;

pub fn render(frame: &mut Frame, area: Rect, palette: &Palette, prompt: &Prompt, owned: &[Playlist]) {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    let field = |label: &str, text: &str| {
        Line::from(vec![
            Span::raw(format!("{label}: ")),
            Span::styled(text.to_owned(), bold),
            Span::styled("_", palette.subtitle()),
        ])
    };
    let lines: Vec<Line> = match prompt {
        Prompt::AddTo {
            title,
            selected,
            naming: None,
            ..
        } => {
            let mut lines = vec![
                Line::from(format!("Add \"{title}\" to")),
                Line::from(""),
            ];
            // The window of rows that keeps the selection on screen.
            let rows = 1 + owned.len();
            let first = selected.saturating_sub(LISTED - 1).min(rows.saturating_sub(LISTED));
            for row in first..rows.min(first + LISTED) {
                let text = match row {
                    0 => "New playlist…".to_string(),
                    n => owned[n - 1].title.clone(),
                };
                let (mark, style) = match row == *selected {
                    true => ("> ", Style::default().bg(palette.selection)),
                    false => ("  ", Style::default()),
                };
                lines.push(Line::styled(format!("{mark}{text}"), style));
            }
            lines.push(Line::from(""));
            lines.push(Line::styled(
                "j/k to choose, Enter to pick, Esc to close.",
                palette.subtitle(),
            ));
            lines
        }
        Prompt::AddTo {
            title,
            naming: Some(name),
            ..
        } => vec![
            Line::from(format!("A new playlist for \"{title}\"")),
            Line::from(""),
            field("name", name),
            Line::from(""),
            Line::styled("Enter to create and add, Esc to go back.", palette.subtitle()),
        ],
        Prompt::NewPlaylist { name } => vec![
            Line::from("New playlist"),
            Line::from(""),
            field("name", name),
            Line::from(""),
            Line::styled("Enter to create, Esc to close.", palette.subtitle()),
        ],
        Prompt::Rename { name, .. } => vec![
            Line::from("Rename the playlist"),
            Line::from(""),
            field("name", name),
            Line::from(""),
            Line::styled("Enter to rename, Esc to close.", palette.subtitle()),
        ],
        Prompt::Delete { title, .. } => vec![
            Line::from(format!("Delete \"{title}\"?")),
            Line::from(""),
            Line::from("The service cannot take this back."),
            Line::from(""),
            Line::styled("y to delete, anything else to keep it.", palette.subtitle()),
        ],
    };
    let modal = centred(area, 60, lines.len() as u16 + 2);
    if modal.width < 4 || modal.height < 3 {
        return;
    }
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Left).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(palette.rule())
                .padding(ratatui::widgets::Padding::horizontal(1))
                .title(" Playlist ")
                .title_style(palette.page_heading()),
        ),
        modal,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned() -> Vec<Playlist> {
        let mut a = Playlist::sample("Mine", 3);
        a.uuid = "a".into();
        let mut b = Playlist::sample("Also mine", 9);
        b.uuid = "b".into();
        vec![a, b]
    }

    fn adding() -> Prompt {
        Prompt::AddTo {
            track: TrackId("t1".into()),
            title: "A Song".into(),
            selected: 0,
            naming: None,
        }
    }

    #[test]
    fn picking_a_playlist_adds_to_it_and_the_first_row_makes_a_new_one() {
        let lists = owned();
        let mut p = adding();
        on_key(&mut p, KeyCode::Char('j'), &lists);
        on_key(&mut p, KeyCode::Char('j'), &lists);
        on_key(&mut p, KeyCode::Char('j'), &lists);
        assert!(matches!(p, Prompt::AddTo { selected: 2, .. }), "stops at the last row");
        match on_key(&mut p, KeyCode::Enter, &lists) {
            Outcome::Done(Edit::AddTo { uuid, title, track }) => {
                assert_eq!((uuid.as_str(), title.as_str(), track.as_str()), ("b", "Also mine", "t1"));
            }
            other => panic!("expected an add, got {other:?}"),
        }

        let mut p = adding();
        assert!(matches!(on_key(&mut p, KeyCode::Enter, &lists), Outcome::Kept));
        for c in "Road".chars() {
            on_key(&mut p, KeyCode::Char(c), &lists);
        }
        assert!(matches!(on_key(&mut p, KeyCode::Esc, &lists), Outcome::Kept), "Esc backs out of the name");
        assert!(matches!(p, Prompt::AddTo { naming: None, .. }));
        on_key(&mut p, KeyCode::Enter, &lists);
        for c in "Road".chars() {
            on_key(&mut p, KeyCode::Char(c), &lists);
        }
        match on_key(&mut p, KeyCode::Enter, &lists) {
            Outcome::Done(Edit::Create { name, then_add }) => {
                assert_eq!(name, "Road");
                assert_eq!(then_add.as_ref().map(|t| t.as_str()), Some("t1"));
            }
            other => panic!("expected a create, got {other:?}"),
        }
    }

    #[test]
    fn a_blank_name_is_not_sent_and_a_deletion_needs_a_plain_y() {
        let mut p = Prompt::NewPlaylist { name: "  ".into() };
        assert!(matches!(on_key(&mut p, KeyCode::Enter, &[]), Outcome::Kept));
        let mut p = Prompt::Rename {
            uuid: "a".into(),
            name: "Mine".into(),
        };
        on_key(&mut p, KeyCode::Backspace, &[]);
        on_key(&mut p, KeyCode::Char('x'), &[]);
        assert!(matches!(
            on_key(&mut p, KeyCode::Enter, &[]),
            Outcome::Done(Edit::Rename { name, .. }) if name == "Minx"
        ));
        let mut p = Prompt::Delete {
            uuid: "a".into(),
            title: "Mine".into(),
        };
        assert!(matches!(on_key(&mut p, KeyCode::Enter, &[]), Outcome::Closed), "Enter is not a yes");
        assert!(matches!(
            on_key(&mut p, KeyCode::Char('y'), &[]),
            Outcome::Done(Edit::Delete { uuid }) if uuid == "a"
        ));
    }

    #[test]
    fn the_prompts_draw_what_they_ask() {
        let lists = owned();
        let draw = |p: &Prompt| {
            let buf = crate::shell::geometry::draw(90, 24, |f, a, pal| render(f, a, pal, p, &lists));
            crate::shell::geometry::text(&buf)
        };
        let text = draw(&adding());
        assert!(text.contains("A Song") && text.contains("> New playlist") && text.contains("Also mine"), "{text}");
        let text = draw(&Prompt::Delete {
            uuid: "a".into(),
            title: "Mine".into(),
        });
        assert!(text.contains("Delete \"Mine\"?"), "{text}");
        let text = draw(&Prompt::Rename {
            uuid: "a".into(),
            name: "Mi".into(),
        });
        assert!(text.contains("name: Mi"), "{text}");
    }
}
