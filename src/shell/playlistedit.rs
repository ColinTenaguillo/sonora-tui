//! Editing playlists: a track into one (`P`), a new one (`N`), a rename
//! (`m`), a deletion (`D`). Each asks something first -- which playlist,
//! what name, are you sure -- and the question is a small modal over the
//! view that takes the keys while it is up. Taking a track out (`x`) asks
//! nothing and lives with the other keys.
//!
//! Only the account's own playlists are offered or touched: the service
//! says which those are, and a followed playlist or a mix is read-only.
//!
//! The pick list is the one TIDAL's app shows: a search box, a "create"
//! row, then each playlist with its cover, its name and its size.

use crossterm::event::KeyCode;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::domain::TrackId;
use crate::library::Playlist;

use super::artwork::{Artwork, Shape};
use super::theme::Palette;

/// The question up, and what has been typed or picked so far.
#[derive(Debug, Clone)]
pub enum Prompt {
    /// Which playlist takes the track. Row 0 creates one, named in place
    /// once picked; the rest are the account's own that match `filter`.
    AddTo {
        track: TrackId,
        title: String,
        selected: usize,
        naming: Option<String>,
        filter: String,
        /// Whether keys go into the search box rather than the list.
        typing: bool,
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

impl Prompt {
    /// The pick list for `track`, as `P` opens it.
    pub fn add_to(track: TrackId, title: String) -> Self {
        Prompt::AddTo {
            track,
            title,
            selected: 0,
            naming: None,
            filter: String::new(),
            typing: false,
        }
    }
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

/// The playlists the list shows for `filter`: all of them for none.
fn shown<'a>(owned: &'a [Playlist], filter: &str) -> Vec<&'a Playlist> {
    owned
        .iter()
        .filter(|p| filter.trim().is_empty() || super::fuzzy::matches(filter, &[&p.title]))
        .collect()
}

/// One key into the prompt. `owned` is the account's own playlists, the
/// ones `AddTo` lists.
pub fn on_key(prompt: &mut Prompt, key: KeyCode, owned: &[Playlist]) -> Outcome {
    match prompt {
        Prompt::AddTo {
            track,
            selected,
            naming: naming @ None,
            filter,
            typing,
            ..
        } => {
            let listed = shown(owned, filter).len();
            match key {
                // The list moves from the box too: a search that leaves
                // three rows should not need a second mode to pick one.
                KeyCode::Down => *selected = (*selected + 1).min(listed),
                KeyCode::Up => *selected = selected.saturating_sub(1),
                KeyCode::Char('j') if !*typing => *selected = (*selected + 1).min(listed),
                KeyCode::Char('k') if !*typing => *selected = selected.saturating_sub(1),
                KeyCode::Char('/') if !*typing => *typing = true,
                KeyCode::Char(c) if *typing => {
                    filter.push(c);
                    *selected = 0;
                }
                KeyCode::Backspace if *typing => {
                    filter.pop();
                    *selected = 0;
                }
                KeyCode::Enter => match *selected {
                    0 => *naming = Some(String::new()),
                    n => {
                        return match shown(owned, filter).get(n - 1) {
                            Some(p) => Outcome::Done(Edit::AddTo {
                                uuid: p.uuid.clone(),
                                title: p.title.clone(),
                                track: track.clone(),
                            }),
                            None => Outcome::Closed,
                        };
                    }
                },
                // Esc backs out one step: the box, then the search, then
                // the prompt.
                KeyCode::Esc if *typing => *typing = false,
                KeyCode::Esc if !filter.is_empty() => {
                    filter.clear();
                    *selected = 0;
                }
                KeyCode::Esc => return Outcome::Closed,
                _ => {}
            }
            Outcome::Kept
        }
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

/// The pick list's shape: each row is two lines, a cover beside them.
const ROW: u16 = 2;
const COVER_COLS: u16 = 5;
const WIDTH: u16 = 62;
/// Title, blank, the search box, blank.
const HEAD: u16 = 1 + 1 + 3 + 1;

fn frame_block(palette: &Palette) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(palette.rule())
        .padding(ratatui::widgets::Padding::horizontal(1))
        .title(" Playlist ")
        .title_style(palette.page_heading())
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
    prompt: &Prompt,
    owned: &[Playlist],
    artwork: Option<&mut Artwork>,
) {
    let bold = Style::default().add_modifier(Modifier::BOLD);
    if let Prompt::AddTo {
        title,
        selected,
        naming: None,
        filter,
        typing,
        ..
    } = prompt
    {
        return render_list(
            frame, area, palette, title, *selected, filter, *typing, owned, artwork,
        );
    }
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
        Prompt::AddTo { .. } => unreachable!("drawn above"),
    };
    let modal = centred(area, WIDTH, lines.len() as u16 + 2);
    if modal.width < 4 || modal.height < 3 {
        return;
    }
    frame.render_widget(Clear, modal);
    frame.render_widget(
        Paragraph::new(lines)
            .alignment(Alignment::Left)
            .block(frame_block(palette)),
        modal,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_list(
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
    title: &str,
    selected: usize,
    filter: &str,
    typing: bool,
    owned: &[Playlist],
    mut artwork: Option<&mut Artwork>,
) {
    let listed = shown(owned, filter);
    let rows = 1 + listed.len();
    // As tall as the rows want, within the screen; the hint takes a line.
    let wanted = HEAD + rows as u16 * ROW + 1 + 2;
    let modal = centred(area, WIDTH, wanted.min(area.height.saturating_sub(2)));
    if modal.width < 12 || modal.height < HEAD + ROW + 3 {
        return;
    }
    frame.render_widget(Clear, modal);
    let block = frame_block(palette);
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let heading = Paragraph::new(Line::from(format!("Add \"{title}\" to a playlist")));
    frame.render_widget(heading, Rect { height: 1, ..inner });
    super::inputbox::render(
        frame,
        Rect {
            y: inner.y + 2,
            height: 3,
            ..inner
        },
        palette,
        "Search, with /",
        filter,
        typing,
    );

    // The window of rows that keeps the selection on screen.
    let body = Rect {
        y: inner.y + HEAD,
        height: inner.height.saturating_sub(HEAD + 1),
        ..inner
    };
    let fit = (body.height / ROW).max(1) as usize;
    let first = selected.saturating_sub(fit - 1).min(rows.saturating_sub(fit));
    for (slot, row) in (first..rows.min(first + fit)).enumerate() {
        let y = body.y + slot as u16 * ROW;
        let cover = Rect {
            x: body.x,
            y,
            width: COVER_COLS,
            height: ROW,
        };
        let text_area = Rect {
            x: body.x + COVER_COLS + 1,
            y,
            width: body.width.saturating_sub(COVER_COLS + 1),
            height: ROW,
        };
        let style = match row == selected {
            true => Style::default().bg(palette.selection),
            false => Style::default(),
        };
        let (name, detail) = match row {
            0 => ("Create a playlist".to_string(), String::new()),
            n => {
                let p = listed[n - 1];
                (
                    p.title.clone(),
                    super::carousel::collection_detail(p.track_count, p.duration),
                )
            }
        };
        let drawn = match row {
            0 => false,
            n => match (&listed[n - 1].cover, artwork.as_deref_mut()) {
                (Some(url), Some(art)) => art.render_shaped(frame, cover, url, Shape::Square),
                _ => false,
            },
        };
        if !drawn {
            // A plus for the create row, a blank sleeve for a playlist
            // whose cover has not arrived.
            let glyph = match row {
                0 => " + ",
                _ => "   ",
            };
            frame.render_widget(
                Paragraph::new(vec![
                    Line::styled(glyph, palette.accent_text().add_modifier(Modifier::BOLD)),
                    Line::from(""),
                ])
                .alignment(Alignment::Center),
                cover,
            );
        }
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(name, style.add_modifier(Modifier::BOLD)),
                Line::styled(detail, style.patch(palette.subtitle())),
            ]),
            text_area,
        );
    }
    let hint = match typing {
        true => "type to search; up/down to move, Enter to pick, Esc to leave the box",
        false => "j/k or arrows to move, Enter to pick, / to search, Esc to close",
    };
    frame.render_widget(
        Paragraph::new(Line::styled(hint, palette.subtitle())),
        Rect {
            y: inner.y + inner.height.saturating_sub(1),
            height: 1,
            ..inner
        },
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
        Prompt::add_to(TrackId("t1".into()), "A Song".into())
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
    fn the_search_box_narrows_the_list_and_esc_backs_out_a_step_at_a_time() {
        let lists = owned();
        let mut p = adding();
        on_key(&mut p, KeyCode::Char('/'), &lists);
        for c in "also".chars() {
            on_key(&mut p, KeyCode::Char(c), &lists);
        }
        assert!(matches!(&p, Prompt::AddTo { filter, typing: true, .. } if filter == "also"));
        // Down works from the box, and the one match is row 1.
        on_key(&mut p, KeyCode::Down, &lists);
        on_key(&mut p, KeyCode::Down, &lists);
        assert!(matches!(p, Prompt::AddTo { selected: 1, .. }), "one match, so one row past create");
        assert!(matches!(
            on_key(&mut p, KeyCode::Enter, &lists),
            Outcome::Done(Edit::AddTo { uuid, .. }) if uuid == "b"
        ));

        let mut p = adding();
        on_key(&mut p, KeyCode::Char('/'), &lists);
        on_key(&mut p, KeyCode::Char('m'), &lists);
        assert!(matches!(on_key(&mut p, KeyCode::Esc, &lists), Outcome::Kept), "out of the box");
        assert!(matches!(&p, Prompt::AddTo { typing: false, filter, .. } if filter == "m"));
        assert!(matches!(on_key(&mut p, KeyCode::Esc, &lists), Outcome::Kept), "the search cleared");
        assert!(matches!(&p, Prompt::AddTo { filter, .. } if filter.is_empty()));
        assert!(matches!(on_key(&mut p, KeyCode::Esc, &lists), Outcome::Closed));
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
            let buf = crate::shell::geometry::draw(90, 30, |f, a, pal| render(f, a, pal, p, &lists, None));
            crate::shell::geometry::text(&buf)
        };
        let text = draw(&adding());
        assert!(text.contains("A Song"), "{text}");
        assert!(text.contains("Create a playlist") && text.contains("Also mine"), "{text}");
        assert!(text.contains("9 tracks"), "a row says how big the playlist is:\n{text}");
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
