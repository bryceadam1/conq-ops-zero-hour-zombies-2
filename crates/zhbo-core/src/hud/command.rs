//! The first-person command line (docs/COMMANDS.md section 8): the line being typed, at the lower
//! left where BO2 puts chat, and under it the echo of the last command (what was done, or why
//! not), fading out over its last second.

use super::Prim;
use crate::command::session::{Session, ECHO_TIME};

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
    if let Some(echo) = &session.echo {
        let alpha = ((ECHO_TIME - echo.age).clamp(0.0, 1.0) * 255.0) as u32;
        let rgb = if echo.ok { DONE } else { REFUSED };
        prims.push(Prim::Text { x, y: y + 26.0 * s, text: echo.text.clone(), argb: (alpha << 24) | rgb, size, align: 0 });
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
