//! The first-person command line and command menu (docs/COMMANDS.md section 8): the line being
//! typed, or the menu's command so far with its options, at the lower left where BO2 puts chat; and
//! under it the echo of the last command (what was done, or why not), fading out over its last
//! second.

use super::Prim;
use crate::command::session::{Menu, Session, ECHO_TIME};

/// How many of the menu's options show at once, around the highlighted one.
pub const MENU_ROWS: usize = 7;

/// The echo's colours: done, refused.
const DONE: u32 = 0x0080_FF80;
const REFUSED: u32 = 0x00FF_8060;

pub fn draw(prims: &mut Vec<Prim>, w: f32, h: f32, s: f32, session: &Session) {
    let x = 24.0 * s;
    let y = h * 0.66;
    let size = (14.0 * s) as i32;
    if session.open {
        let line_w = (w * 0.45).max(320.0 * s);
        prims.push(Prim::Rect { x: x - 6.0 * s, y: y - 4.0 * s, w: line_w, h: 22.0 * s, argb: 0xA000_0000 });
        prims.push(Prim::Text { x, y, text: format!("> {}_", session.line), argb: 0xFFFF_FFFF, size, align: 0 });
    }
    if let Some(menu) = &session.menu {
        draw_menu(prims, w, x, y, s, size, menu);
    }
    if let Some(echo) = &session.echo {
        let alpha = ((ECHO_TIME - echo.age).clamp(0.0, 1.0) * 255.0) as u32;
        let rgb = if echo.ok { DONE } else { REFUSED };
        prims.push(Prim::Text { x, y: y + 26.0 * s, text: echo.text.clone(), argb: (alpha << 24) | rgb, size, align: 0 });
    }
}

/// The menu: its command so far on the line, its options above it, the highlighted one marked.
fn draw_menu(prims: &mut Vec<Prim>, w: f32, x: f32, y: f32, s: f32, size: i32, menu: &Menu) {
    let line_w = (w * 0.45).max(320.0 * s);
    let row = 20.0 * s;
    prims.push(Prim::Rect { x: x - 6.0 * s, y: y - 4.0 * s, w: line_w, h: 22.0 * s, argb: 0xA000_0000 });
    prims.push(Prim::Text { x, y, text: format!("> {} _", menu.text()), argb: 0xFFFF_FFFF, size, align: 0 });
    let n = menu.options.len();
    if n == 0 {
        let text = if menu.dirty { "…" } else { "(nothing fits here)" };
        prims.push(Prim::Text { x, y: y - row, text: text.into(), argb: 0xC0C0_C0C0, size, align: 0 });
        return;
    }
    // A window of rows around the highlight, the first option on top.
    let first = menu.highlight.saturating_sub(MENU_ROWS / 2).min(n.saturating_sub(MENU_ROWS));
    let shown: Vec<usize> = (first..n.min(first + MENU_ROWS)).collect();
    let top = y - row * shown.len() as f32 - 6.0 * s;
    prims.push(Prim::Rect { x: x - 6.0 * s, y: top - 2.0 * s, w: line_w * 0.7, h: row * shown.len() as f32 + 4.0 * s, argb: 0x9000_0000 });
    for (k, &i) in shown.iter().enumerate() {
        let o = &menu.options[i];
        let yy = top + row * k as f32;
        let selected = i == menu.highlight;
        if selected {
            prims.push(Prim::Rect { x: x - 6.0 * s, y: yy - 1.0 * s, w: line_w * 0.7, h: row, argb: 0x80FF_C040 });
        }
        let text = match o.count {
            Some(c) => format!("{} ({c})", o.text),
            None => o.text.clone(),
        };
        prims.push(Prim::Text { x, y: yy, text, argb: if selected { 0xFFFF_FFFF } else { 0xFFC8_C8C8 }, size, align: 0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::session::Echo;

    fn texts(prims: &[Prim]) -> Vec<(String, u32)> {
        prims.iter().filter_map(|p| if let Prim::Text { text, argb, .. } = p { Some((text.clone(), *argb)) } else { None }).collect()
    }

    #[test]
    fn the_menu_shows_its_command_and_a_window_of_options_around_the_highlight() {
        use crate::command::session::MenuOption;
        let mut session = Session::default();
        let options: Vec<MenuOption> = (0..12).map(|i| MenuOption { text: format!("option {i}"), count: (i == 9).then_some(3) }).collect();
        session.menu = Some(Menu { picks: vec!["rangers".into()], options, highlight: 9, dirty: false });
        let mut prims = Vec::new();
        draw(&mut prims, 1280.0, 720.0, 1.0, &session);
        let t: Vec<String> = texts(&prims).into_iter().map(|(t, _)| t).collect();
        assert_eq!(t[0], "> rangers _");
        // Seven rows, the highlight (9) in the middle where the list allows: 5..=11.
        assert_eq!(&t[1..], ["option 5", "option 6", "option 7", "option 8", "option 9 (3)", "option 10", "option 11"]);
    }

    #[test]
    fn the_open_line_shows_what_is_typed_and_the_echo_fades() {
        let mut session = Session::default();
        session.open = true;
        session.line = "rangers move".into();
        let mut prims = Vec::new();
        draw(&mut prims, 1280.0, 720.0, 1.0, &session);
        assert_eq!(texts(&prims), vec![("> rangers move_".to_string(), 0xFFFF_FFFF)]);

        session.open = false;
        session.echo = Some(Echo { text: "3 units: rangers move to me".into(), ok: true, age: 0.0 });
        prims.clear();
        draw(&mut prims, 1280.0, 720.0, 1.0, &session);
        assert_eq!(texts(&prims), vec![("3 units: rangers move to me".to_string(), 0xFF00_0000 | DONE)]);

        session.echo = Some(Echo { text: "rangres stop: didn't expect 'rangres'".into(), ok: false, age: ECHO_TIME - 0.5 });
        prims.clear();
        draw(&mut prims, 1280.0, 720.0, 1.0, &session);
        let (_, argb) = &texts(&prims)[0];
        assert_eq!(argb & 0x00FF_FFFF, REFUSED);
        assert_eq!(argb >> 24, 127, "half a second left: half faded");
    }
}
