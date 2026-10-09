//! Turns solved boxes into shapes, in draw order.

use super::{Content, Layout, Node, Paint, Text};
use crate::canvas::{Canvas, Color, Glyphs, Quad, Rect, Vertex};

impl Layout {
    /// Draws each box's background, border and content into `canvas`: the
    /// tree in declaration order, then each floating box over it.
    pub fn emit(&mut self, canvas: &mut Canvas, glyphs: &mut Glyphs) {
        let Some(window) = self.nodes.first() else {
            return;
        };
        let root = window.rect;
        self.floats.clear();
        self.emit_tree(0, root, canvas, glyphs);
        // A floating box inside a floating box joins the list as it's drawn.
        let mut next = 0;
        while let Some(&float) = self.floats.get(next) {
            self.emit_tree(float, root, canvas, glyphs);
            next += 1;
        }
    }

    /// Draws `top` and what's under it, clipped to `clip`, leaving its
    /// floating boxes for later.
    fn emit_tree(&mut self, top: usize, clip: Rect, canvas: &mut Canvas, glyphs: &mut Glyphs) {
        canvas.clip(clip);
        self.clips.clear();
        let mut current = clip;
        let end = self.nodes[top].end;
        let mut index = top;
        while index < end {
            while let Some(&(until, outer)) = self.clips.last()
                && index >= until
            {
                self.clips.pop();
                current = outer;
                canvas.clip(current);
            }
            let node = &self.nodes[index];
            if index != top && node.element.float.is_some() {
                self.floats.push(index);
                index = node.end;
                continue;
            }
            let drawn = Drawn {
                paints: &self.paints,
                vertices: &self.vertices,
                indices: &self.indices,
            };
            draw(node, &mut self.texts, &drawn, canvas, glyphs);
            if node.element.clip && node.end > index + 1 {
                self.clips.push((node.end, current));
                current = current.intersect(node.rect);
                canvas.clip(current);
            }
            index += 1;
        }
    }
}

/// What custom boxes drew.
struct Drawn<'a> {
    paints: &'a [Paint],
    vertices: &'a [Vertex],
    indices: &'a [u32],
}

fn draw(node: &Node, texts: &mut [Text], drawn: &Drawn, canvas: &mut Canvas, glyphs: &mut Glyphs) {
    let element = &node.element;
    let rect = node.rect;
    if element.background.is_some() || element.border.is_some() {
        let fill = element.background.unwrap_or(Color::TRANSPARENT);
        let mut quad = Quad::new(rect, fill).rounded(element.radius);
        if let Some(border) = element.border {
            quad = quad.bordered(border.width, border.color);
        }
        canvas.quad(quad);
    }
    match &node.content {
        Content::Box => {}
        Content::Text(at) => {
            if let Some(text) = texts.get_mut(*at) {
                let baseline = [rect.x, rect.y + text.ascent];
                glyphs.draw_line(canvas, &mut text.line, baseline, text.style.color);
            }
        }
        Content::Icon(icon, size, color) => {
            glyphs.icon(canvas, *icon, [rect.x, rect.y], *size, *color);
        }
        Content::Custom(range) => {
            for paint in drawn.paints.get(range.clone()).unwrap_or_default() {
                match paint {
                    Paint::Quad(quad) => canvas.quad(*quad),
                    Paint::Triangles { vertices, indices } => {
                        if let (Some(vertices), Some(indices)) = (
                            drawn.vertices.get(vertices.clone()),
                            drawn.indices.get(indices.clone()),
                        ) {
                            canvas.triangles(vertices, indices);
                        }
                    }
                }
            }
        }
    }
}
