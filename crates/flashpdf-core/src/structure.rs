//! The structure tree of tagged output. The protocol opens and closes elements
//! with roles the compiler chooses; painting attaches marked content and link
//! annotations to them.

use pdf_writer::{Content, Name};

use crate::pdf::pair;

/// Content in an `Artifact` element paints as an artifact and joins no structure element.
pub(crate) const ARTIFACT: u32 = u32::MAX;
/// Structure elements per render; each is a PDF object.
const MAX_ELEMENTS: usize = 1_000_000;
/// Open structure elements; deeper than any element tree the compiler accepts.
const MAX_DEPTH: usize = 256;

/// The standard structure types of ISO 32000-1, 14.8.4, which PDF/UA accepts without a role map.
const ROLES: &str = concat!(
    "Document Part Art Sect Div BlockQuote Caption TOC TOCI Index NonStruct Private ",
    "P H H1 H2 H3 H4 H5 H6 L LI Lbl LBody Table TR TH TD THead TBody TFoot ",
    "Span Quote Note Reference BibEntry Code Link Annot Ruby RB RT RP Warichu WT WP ",
    "Figure Formula Form",
);

pub(crate) struct Element {
    pub(crate) role: &'static str,
    pub(crate) parent: u32,
    pub(crate) alt: String,
    /// Table attributes: scope (0 none, 1 Row, 2 Column, 3 Both), row span, column span.
    pub(crate) table: [u16; 3],
    /// Where its image painted, rounded out to whole points, for a Figure's layout attribute.
    pub(crate) bbox: Option<[i32; 4]>,
    pub(crate) kids: Vec<Kid>,
}

pub(crate) enum Kid {
    Element(u32),
    /// Page index and marked-content id.
    Content(usize, i32),
    /// Page index and the annotation's index in the document.
    Annotation(usize, usize),
}

impl Kid {
    pub(crate) fn page(&self) -> Option<usize> {
        match self {
            Self::Element(_) => None,
            Self::Content(page, _) | Self::Annotation(page, _) => Some(*page),
        }
    }
}

pub(crate) struct Structure {
    /// Element 0 is the Document, the structure tree root's only child.
    pub(crate) elements: Vec<Element>,
    /// Per page, the structure element of each marked-content id.
    pub(crate) pages: Vec<Vec<u32>>,
    /// Link annotations painted so far.
    pub(crate) annotations: usize,
    open: Vec<u32>,
}

impl Default for Structure {
    fn default() -> Self {
        let mut structure = Self {
            elements: Vec::new(),
            pages: vec![Vec::new()],
            annotations: 0,
            open: Vec::new(),
        };
        structure.push("Document", 0);
        structure
    }
}

impl Structure {
    /// The element content decoded now belongs to.
    pub(crate) fn current(&self) -> u32 {
        self.open.last().copied().unwrap_or(0)
    }

    pub(crate) fn is_open(&self) -> bool {
        !self.open.is_empty()
    }

    /// Opens a child of the current element. Inside an artifact, every element is one.
    pub(crate) fn open(
        &mut self,
        role: &[u8],
        alt: String,
        table: [u16; 3],
    ) -> Result<(), &'static str> {
        if self.open.len() >= MAX_DEPTH {
            return Err("structure depth");
        }
        let parent = self.current();
        let element = match ROLES.split(' ').find(|known| known.as_bytes() == role) {
            Some(role) if parent != ARTIFACT => {
                if self.elements.len() >= MAX_ELEMENTS {
                    return Err("structure element count");
                }
                let element = self.push(role, parent);
                let last = self.elements.last_mut().unwrap();
                last.alt = alt;
                last.table = table;
                element
            }
            None if role != b"Artifact" => return Err("structure role"),
            _ => ARTIFACT,
        };
        self.open.push(element);
        Ok(())
    }

    pub(crate) fn close(&mut self) -> Result<(), &'static str> {
        self.open.pop().map(drop).ok_or("structure end")
    }

    fn push(&mut self, role: &'static str, parent: u32) -> u32 {
        let index = self.elements.len() as u32;
        if let Some(parent) = self.elements.get_mut(parent as usize) {
            parent.kids.push(Kid::Element(index));
        }
        self.elements.push(Element {
            role,
            parent,
            alt: String::new(),
            table: [0, 1, 1],
            bbox: None,
            kids: Vec::new(),
        });
        index
    }

    /// A Link element for an annotation over content of `parent`; none inside an artifact.
    pub(crate) fn link(&mut self, parent: u32) -> Option<u32> {
        (parent != ARTIFACT).then(|| self.push("Link", parent))
    }

    /// Begins marked content for `element` on the last page: an artifact when the
    /// paint repeats or the element is one, otherwise the element's next MCID.
    pub(crate) fn mark(&mut self, content: &mut Content, element: u32, repeat: bool) {
        if repeat || element == ARTIFACT {
            content.begin_marked_content(Name(b"Artifact"));
            return;
        }
        let page = self.pages.len() - 1;
        let ids = &mut self.pages[page];
        let id = ids.len() as i32;
        ids.push(element);
        let element = &mut self.elements[element as usize];
        element.kids.push(Kid::Content(page, id));
        let mut marked =
            content.begin_marked_content_with_properties(Name(element.role.as_bytes()));
        pair(&mut marked.properties(), b"MCID", id);
    }

    /// Counts the annotation being painted and gives it to its Link element.
    pub(crate) fn annotate(&mut self, link: Option<u32>) {
        if let Some(link) = link {
            let page = self.pages.len() - 1;
            self.elements[link as usize]
                .kids
                .push(Kid::Annotation(page, self.annotations));
        }
        self.annotations += 1;
    }
}
