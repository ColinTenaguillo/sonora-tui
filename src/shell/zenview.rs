//! Zen: the playing track alone. Its cover as large as the screen allows,
//! its title and artist under it, centred; the bar below is the app's own.

use ratatui::layout::{Alignment, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::theme::Palette;
use crate::domain::Track;

/// A blank row above the cover, one between it and the title, the title,
/// the artist, a blank row above the bar.
const CHROME_ROWS: u16 = 5;

/// Where the cover goes in a pane of `area`: the tallest square that fits,
/// half the pane at most -- as the web client's full-screen player, the
/// cover is the subject, not a wall -- with the block of cover, gap and two
/// lines centred. No height when there is no room for one.
pub fn cover_area(area: Rect) -> Rect {
    let mut rows = (area.height / 2).min(area.height.saturating_sub(CHROME_ROWS));
    while rows > 0 && super::carousel::square_width(rows) > area.width {
        rows -= 1;
    }
    let cols = if rows == 0 {
        0
    } else {
        super::carousel::square_width(rows)
    };
    let top = area.y + area.height.saturating_sub(rows + CHROME_ROWS - 2) / 2;
    Rect {
        x: area.x + (area.width - cols) / 2,
        y: top,
        width: cols,
        height: rows,
    }
}

pub fn render<F>(
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
    track: Option<&Track>,
    liked: bool,
    mut draw_cover: F,
) where
    F: FnMut(&mut Frame, Rect, &str, super::artwork::Shape) -> bool,
{
    let Some(track) = track else {
        if let Some(middle) = area.rows().nth(usize::from(area.height / 2)) {
            frame.render_widget(
                Paragraph::new(Line::styled("Nothing playing", palette.subtitle()))
                    .alignment(Alignment::Center),
                middle,
            );
        }
        return;
    };

    let cover = cover_area(area);
    super::carousel::cover_or_stand_in(
        frame,
        cover,
        palette,
        track.cover.as_deref(),
        false,
        None,
        &mut draw_cover,
    );

    // A row of air under the cover, then the title and the artist, cut at
    // the bottom of the pane.
    let text = area.intersection(Rect {
        y: cover.bottom() + 1,
        ..area
    });
    let heart = super::carousel::heart(liked);
    frame.render_widget(
        Paragraph::new(vec![
            super::carousel::marked_title(
                &track.title,
                &heart,
                area.width,
                palette.title(),
                palette,
            ),
            Line::styled(track.artist.clone(), palette.subtitle()),
        ])
        .alignment(Alignment::Center),
        text,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::geometry;

    #[test]
    fn a_pane_too_small_for_a_cover_still_names_the_track() {
        let track = Track::sample("Song", "Someone", std::time::Duration::ZERO);
        for (w, h) in [(0, 0), (1, 1), (10, 3), (4, 30)] {
            let buf = geometry::draw(w, h, |f, area, palette| {
                render(f, area, palette, Some(&track), false, |_, _, _, _| false);
            });
            if h >= 3 && w >= 4 {
                assert!(geometry::text(&buf).contains("Song"), "{w}x{h}");
            }
        }
    }

    #[test]
    fn the_cover_is_centred_over_the_title() {
        let track = Track::sample("Song", "Someone", std::time::Duration::ZERO);
        let buf = geometry::draw(80, 30, |f, area, palette| {
            render(f, area, palette, Some(&track), false, |_, _, _, _| false);
        });
        let title = geometry::find(&buf, "Song").expect("title");
        let cover = geometry::occupied(&buf, title.row - 2).expect("the cover's last row");
        assert!(
            geometry::occupied(&buf, title.row - 1).is_none(),
            "a row of air between them:\n{}",
            geometry::text(&buf)
        );
        geometry::assert_centred_on(cover, title, &buf);
        geometry::assert_centred(cover, &buf);

        // And the block of cover, air, title and artist centred top to
        // bottom: as many blank rows above it as below, within one.
        let painted: Vec<u16> = (0..30)
            .filter(|y| geometry::occupied(&buf, *y).is_some())
            .collect();
        let (first, last) = (painted[0], *painted.last().unwrap());
        assert!(
            first.abs_diff(29 - last) <= 1,
            "{first} rows above, {} below:\n{}",
            29 - last,
            geometry::text(&buf)
        );
    }

    #[test]
    fn a_pane_exactly_as_wide_as_the_cover_still_gets_it_whole() {
        let pane = Rect::new(0, 0, 0, 40);
        let wide = cover_area(Rect { width: 400, ..pane });
        let exact = cover_area(Rect {
            width: wide.width,
            ..pane
        });
        assert_eq!((exact.width, exact.height), (wide.width, wide.height));
        assert_eq!(exact.x, 0, "and fills it from the left edge");
    }

    #[test]
    fn nothing_playing_is_said_in_the_middle() {
        let buf = geometry::draw(40, 11, |f, area, palette| {
            render(f, area, palette, None, false, |_, _, _, _| false);
        });
        let line = geometry::find(&buf, "Nothing playing").expect("said");
        assert_eq!(line.row, 5);
        geometry::assert_centred(line, &buf);
    }

    #[test]
    fn the_cover_takes_no_more_than_half_the_pane() {
        let track = Track::sample("Song", "Someone", std::time::Duration::ZERO);
        let buf = geometry::draw(200, 40, |f, area, palette| {
            render(f, area, palette, Some(&track), false, |_, _, _, _| false);
        });
        let rows = (0..40)
            .filter(|y| {
                geometry::occupied(&buf, *y).is_some_and(|s| s.width() > "Someone".len() as u16)
            })
            .count();
        assert!((1..=20).contains(&rows), "{rows} rows of cover");
    }
}
