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

/// The page body: the page minus its margins and the header and footer it paints.
#[derive(Clone, Copy)]
pub(crate) struct PageLayout {
    page: Page,
    pub(crate) header: Pt,
    pub(crate) footer: Pt,
}

impl PageLayout {
    pub(crate) fn new(page: Page) -> Self {
        Self {
            page,
            header: Pt::ZERO,
            footer: Pt::ZERO,
        }
    }

    pub(crate) fn page(self) -> Page {
        self.page
    }

    pub(crate) fn content_width(self) -> Pt {
        self.page.width - self.page.margin.left - self.page.margin.right
    }

    pub(crate) fn content_height(self) -> Pt {
        self.top() - self.bottom()
    }

    pub(crate) fn top(self) -> Pt {
        self.page.height - self.page.margin.top - self.header
    }

    pub(crate) fn bottom(self) -> Pt {
        self.page.margin.bottom + self.footer
    }

    pub(crate) fn left(self) -> Pt {
        self.page.margin.left
    }

    /// The header and footer may take at most half the height between the margins.
    pub(crate) fn validate_furniture(self) -> Result<(), RenderError> {
        let between = self.page.height - self.page.margin.top - self.page.margin.bottom;
        if (self.header + self.footer).get() <= between.get() / 2.0 {
            Ok(())
        } else {
            Err(RenderError::PageOverflow)
        }
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
        height <= self.remaining(page)
    }

    pub(crate) fn remaining(&self, page: PageLayout) -> Pt {
        self.y - page.bottom()
    }

    pub(crate) fn at_top(&self, page: PageLayout) -> bool {
        self.y == page.top()
    }

    pub(crate) fn origin(&self, page: PageLayout) -> Point {
        Point {
            x: page.left(),
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
