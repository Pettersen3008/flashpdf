use crate::{Page, Pt, RenderError};

#[derive(Clone, Copy, Debug)]
pub(crate) struct Point {
    pub(crate) x: Pt,
    pub(crate) y: Pt,
}

impl Point {
    pub(crate) const ZERO: Self = Self {
        x: Pt::ZERO,
        y: Pt::ZERO,
    };

    pub(crate) fn translated(self, dx: Pt, dy: Pt) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LayoutArea {
    pub(crate) origin: Point,
    pub(crate) width: Pt,
}

impl LayoutArea {
    pub(crate) fn root(width: Pt) -> Self {
        Self {
            origin: Point::ZERO,
            width,
        }
    }

    pub(crate) fn translated(self, dx: Pt, dy: Pt) -> Self {
        Self {
            origin: Point {
                x: self.origin.x + dx,
                y: self.origin.y + dy,
            },
            ..self
        }
    }

    pub(crate) fn with_width(self, width: Pt) -> Self {
        Self { width, ..self }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct PageLayout(Page);

impl PageLayout {
    pub(crate) fn new(page: Page) -> Self {
        Self(page)
    }

    pub(crate) fn page(self) -> Page {
        self.0
    }

    pub(crate) fn content_width(self) -> Pt {
        self.0.width - self.0.margin - self.0.margin
    }

    pub(crate) fn content_height(self) -> Pt {
        self.0.height - self.0.margin - self.0.margin
    }

    pub(crate) fn top(self) -> Pt {
        self.0.height - self.0.margin
    }

    pub(crate) fn margin(self) -> Pt {
        self.0.margin
    }

    pub(crate) fn validate_block(self, height: Pt) -> Result<(), RenderError> {
        if height.get().is_finite() && height <= self.content_height() {
            Ok(())
        } else {
            Err(RenderError::PageOverflow)
        }
    }
}

pub(crate) struct PageCursor {
    y: Pt,
}

impl PageCursor {
    pub(crate) fn new(page: PageLayout) -> Self {
        Self { y: page.top() }
    }

    pub(crate) fn fits(&self, height: Pt, page: PageLayout) -> bool {
        height <= self.y - page.margin()
    }

    pub(crate) fn origin(&self, page: PageLayout) -> Point {
        Point {
            x: page.margin(),
            y: self.y,
        }
    }

    pub(crate) fn advance(&mut self, height: Pt) {
        self.y -= height;
    }

    pub(crate) fn reset(&mut self, page: PageLayout) {
        self.y = page.top();
    }
}
