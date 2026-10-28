use super::{inline_tag, plain, Compiler, Ctx, Error, Result, Run, Sink, Styled};

#[derive(Clone, Debug)]
pub struct StructureNode {
    pub(crate) at: Option<std::rc::Rc<str>>,
    pub(crate) role: &'static str,
    pub(crate) parent: u32,
    pub(crate) children: Vec<u32>,
    pub(crate) alt: String,
    pub(crate) lang: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) columns: u16,
    pub(crate) rows: u16,
}

impl<S: Sink> Compiler<S> {
    pub(super) fn structure_node(&mut self, role: &'static str, parent: u32) -> u32 {
        let id = self.structure.len() as u32 + 1;
        self.structure.push(StructureNode {
            at: None,
            role,
            parent,
            children: Vec::new(),
            alt: String::new(),
            lang: None,
            scope: None,
            columns: 1,
            rows: 1,
        });
        if parent != 0 {
            self.structure[parent as usize - 1].children.push(id);
        }
        id
    }

    pub(super) fn structure_context(&mut self, node: &Styled, context: &Ctx) -> Result<Ctx> {
        let mut next = context.clone();
        if context.structure == 0 {
            return Ok(next);
        }
        let tag = node.el.tag.as_str();
        let role = match tag {
            "h1" => "H1",
            "h2" => "H2",
            "h3" => "H3",
            "h4" => "H4",
            "h5" => "H5",
            "h6" => "H6",
            "p" | "pre" => "P",
            "ul" | "ol" => "L",
            "li" => "LI",
            "table" => "Table",
            "thead" => "THead",
            "tbody" => "TBody",
            "tfoot" => "TFoot",
            "tr" => "TR",
            "th" => "TH",
            "td" => "TD",
            "a" if node.el.href.is_some() => "Link",
            "img" | "svg" => "Figure",
            "hr" | "br" => return Ok(next),
            _ if inline_tag(tag) && node.el.lang.is_none() => return Ok(next),
            _ if inline_tag(tag) => "Span",
            _ => "Div",
        };
        next.structure = self.structure_node(role, context.structure);
        let item = &mut self.structure[next.structure as usize - 1];
        item.at = Some(node.at.clone());
        item.lang = node.el.lang.clone();
        item.columns = node.el.col_span.unwrap_or(1);
        item.rows = node.el.row_span.unwrap_or(1);
        if role == "TH" {
            item.scope = Some(node.el.scope.clone().unwrap_or_else(|| "col".into()));
        }
        if role == "Figure" {
            item.alt = node.el.alt.clone().unwrap_or_default();
        }
        if role == "Link" {
            plain(&node.children, &mut item.alt);
        }
        if role == "LI" {
            next.label = self.structure_node("Lbl", next.structure);
            next.structure = self.structure_node("LBody", next.structure);
        }
        Ok(next)
    }

    pub(super) fn structure_paragraph(&mut self, runs: &mut [Run], parent: u32) {
        if parent == 0 || runs.is_empty() {
            return;
        }
        let role = self.structure[parent as usize - 1].role;
        if matches!(role, "P" | "H1" | "H2" | "H3" | "H4" | "H5" | "H6" | "Span") {
            self.structure_inline_order(runs, parent);
            return;
        }
        let id = self.structure_node("P", parent);
        let mut children = Vec::new();
        for run in runs.iter_mut() {
            if run.structure == parent {
                run.structure = id;
                continue;
            }
            let mut child = run.structure;
            while child != 0 && self.structure[child as usize - 1].parent != parent {
                child = self.structure[child as usize - 1].parent;
            }
            if child != 0 && child != id && !children.contains(&child) {
                children.push(child);
            }
        }
        for child in &children {
            self.structure[*child as usize - 1].parent = id;
        }
        let siblings = &mut self.structure[parent as usize - 1].children;
        let at = siblings
            .iter()
            .position(|child| children.contains(child))
            .unwrap_or(siblings.len() - 1);
        siblings.retain(|child| *child != id && !children.contains(child));
        siblings.insert(at.min(siblings.len()), id);
        self.structure[id as usize - 1].children = children;
        self.structure_inline_order(runs, id);
    }
    fn structure_inline_order(&mut self, runs: &mut [Run], parent: u32) {
        let mut last = None;
        for run in runs.iter_mut() {
            let id = run.structure;
            if id != 0 && !self.structure[id as usize - 1].children.is_empty() {
                let span = match last {
                    Some((previous, span)) if previous == id => span,
                    _ => self.structure_node("Span", id),
                };
                run.structure = span;
                last = Some((id, span));
            } else {
                last = None;
            }
        }
        let mut ordered: std::collections::HashMap<u32, Vec<u32>> =
            std::collections::HashMap::new();
        for run in runs {
            let mut child = run.structure;
            while child != 0 && child != parent {
                let ancestor = self.structure[child as usize - 1].parent;
                if ancestor == 0 {
                    break;
                }
                let children = ordered.entry(ancestor).or_default();
                if !children.contains(&child) {
                    children.push(child);
                }
                child = ancestor;
            }
        }
        for (id, children) in ordered {
            self.structure[id as usize - 1].children = children;
        }
    }
    pub(super) fn validate_headings(&self) -> Result<()> {
        let mut pending = vec![1_u32];
        let mut previous = b'0';
        while let Some(id) = pending.pop() {
            let node = &self.structure[id as usize - 1];
            if node.role.starts_with('H') && node.role.len() == 2 {
                let level = node.role.as_bytes()[1];
                if level > previous + 1 {
                    return Err(Error::new(
                        "PDF/UA headings must start at h1 and cannot skip a level",
                    )
                    .locate(node.at.as_deref().unwrap_or("document")));
                }
                previous = level;
            }
            pending.extend(node.children.iter().rev());
        }
        Ok(())
    }
}
