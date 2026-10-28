use super::Key;
use crate::css::Message;

struct Definition {
    name: &'static str,
    css: &'static str,
    inherited: bool,
    unsupported: Option<&'static str>,
}

const fn plain(name: &'static str, css: &'static str) -> Definition {
    Definition {
        name,
        css,
        inherited: false,
        unsupported: None,
    }
}
const fn inherited(name: &'static str, css: &'static str) -> Definition {
    Definition {
        name,
        css,
        inherited: true,
        unsupported: None,
    }
}
const fn rejected(name: &'static str, css: &'static str, reason: &'static str) -> Definition {
    Definition {
        name,
        css,
        inherited: false,
        unsupported: Some(reason),
    }
}

const NO_OUTLINE: &str = "outlines are not drawn; use a border";

/// Every property in the subset by its style-object name and CSS name. Order matters only for
/// the index a `Prop` holds.
static PROPERTIES: &[Definition] = &[
    plain("display", "display"),
    plain("flexDirection", "flex-direction"),
    plain("flexWrap", "flex-wrap"),
    plain("justifyContent", "justify-content"),
    plain("alignItems", "align-items"),
    plain("alignSelf", "align-self"),
    plain("alignContent", "align-content"),
    plain("flex", "flex"),
    plain("flexGrow", "flex-grow"),
    plain("flexShrink", "flex-shrink"),
    plain("flexBasis", "flex-basis"),
    plain("width", "width"),
    plain("minWidth", "min-width"),
    plain("maxWidth", "max-width"),
    plain("height", "height"),
    plain("minHeight", "min-height"),
    plain("maxHeight", "max-height"),
    plain("boxSizing", "box-sizing"),
    plain("margin", "margin"),
    plain("marginTop", "margin-top"),
    plain("marginRight", "margin-right"),
    plain("marginBottom", "margin-bottom"),
    plain("marginLeft", "margin-left"),
    plain("padding", "padding"),
    plain("paddingTop", "padding-top"),
    plain("paddingRight", "padding-right"),
    plain("paddingBottom", "padding-bottom"),
    plain("paddingLeft", "padding-left"),
    plain("border", "border"),
    plain("borderWidth", "border-width"),
    plain("borderColor", "border-color"),
    plain("borderTop", "border-top"),
    plain("borderRight", "border-right"),
    plain("borderBottom", "border-bottom"),
    plain("borderLeft", "border-left"),
    plain("borderStyle", "border-style"),
    plain("borderTopWidth", "border-top-width"),
    plain("borderRightWidth", "border-right-width"),
    plain("borderBottomWidth", "border-bottom-width"),
    plain("borderLeftWidth", "border-left-width"),
    plain("borderTopColor", "border-top-color"),
    plain("borderRightColor", "border-right-color"),
    plain("borderBottomColor", "border-bottom-color"),
    plain("borderLeftColor", "border-left-color"),
    plain("borderTopStyle", "border-top-style"),
    plain("borderRightStyle", "border-right-style"),
    plain("borderBottomStyle", "border-bottom-style"),
    plain("borderLeftStyle", "border-left-style"),
    plain("borderRadius", "border-radius"),
    plain("borderTopLeftRadius", "border-top-left-radius"),
    plain("borderTopRightRadius", "border-top-right-radius"),
    plain("borderBottomRightRadius", "border-bottom-right-radius"),
    plain("borderBottomLeftRadius", "border-bottom-left-radius"),
    // Not inherited: the compiler multiplies it down the tree.
    plain("opacity", "opacity"),
    plain("marginInline", "margin-inline"),
    plain("marginBlock", "margin-block"),
    plain("paddingInline", "padding-inline"),
    plain("paddingBlock", "padding-block"),
    plain("background", "background"),
    plain("backgroundColor", "background-color"),
    plain("gap", "gap"),
    plain("rowGap", "row-gap"),
    plain("columnGap", "column-gap"),
    inherited("fontSize", "font-size"),
    inherited("fontFamily", "font-family"),
    inherited("fontWeight", "font-weight"),
    inherited("fontStyle", "font-style"),
    inherited("fontStretch", "font-stretch"),
    inherited("fontVariationSettings", "font-variation-settings"),
    inherited("fontKerning", "font-kerning"),
    inherited("textAlign", "text-align"),
    inherited("direction", "direction"),
    inherited("textTransform", "text-transform"),
    // CSS propagates decoration to descendants rather than inheriting it; the effect is the same here.
    inherited("textDecoration", "text-decoration"),
    inherited("textDecorationColor", "text-decoration-color"),
    inherited("whiteSpace", "white-space"),
    // Not inherited: the compiler accumulates the shift down nested inline elements.
    plain("verticalAlign", "vertical-align"),
    // Inherited so a child sees the shorthand in its cascade order, which decides against longhands.
    inherited("listStyle", "list-style"),
    inherited("listStyleType", "list-style-type"),
    inherited("listStylePosition", "list-style-position"),
    inherited("color", "color"),
    // Inherits through the compiler's text state: a factor stays a factor, a length stays a length.
    plain("lineHeight", "line-height"),
    // Inherit through the text state too, so `em` resolves once, against the declaring element.
    plain("letterSpacing", "letter-spacing"),
    plain("wordSpacing", "word-spacing"),
    plain("position", "position"),
    plain("top", "top"),
    plain("right", "right"),
    plain("bottom", "bottom"),
    plain("left", "left"),
    plain("inset", "inset"),
    plain("insetInline", "inset-inline"),
    plain("insetBlock", "inset-block"),
    plain("zIndex", "z-index"),
    plain("breakBefore", "break-before"),
    plain("breakAfter", "break-after"),
    plain("breakInside", "break-inside"),
    plain("boxShadow", "box-shadow"),
    rejected("outline", "outline", NO_OUTLINE),
    rejected("outlineColor", "outline-color", NO_OUTLINE),
    rejected("outlineOffset", "outline-offset", NO_OUTLINE),
    rejected("outlineStyle", "outline-style", NO_OUTLINE),
    rejected("outlineWidth", "outline-width", NO_OUTLINE),
    rejected(
        "borderImage",
        "border-image",
        "border images are not drawn; use a solid, dashed, or dotted border",
    ),
    plain("backgroundImage", "background-image"),
    plain("backgroundSize", "background-size"),
    plain("backgroundPosition", "background-position"),
    inherited("borderCollapse", "border-collapse"),
    inherited("borderSpacing", "border-spacing"),
    inherited("orphans", "orphans"),
    inherited("widows", "widows"),
    rejected(
        "objectFit",
        "object-fit",
        "size an image with width and height; it always scales to fill them",
    ),
    rejected(
        "listStyleImage",
        "list-style-image",
        "markers are text or drawn shapes; choose list-style-type",
    ),
    rejected(
        "boxDecorationBreak",
        "box-decoration-break",
        "a split box draws its top edge on the first page and its bottom edge on the last, as slice does; clone is not implemented",
    ),
];

/// CSS names with no definition of their own. `text-decoration-line` is Tailwind's `underline`;
/// the logical sides assume horizontal left-to-right text.
static ALIASES: &[(&str, &str)] = &[
    ("text-decoration-line", "textDecoration"),
    ("margin-inline-start", "marginLeft"),
    ("margin-inline-end", "marginRight"),
    ("margin-block-start", "marginTop"),
    ("margin-block-end", "marginBottom"),
    ("padding-inline-start", "paddingLeft"),
    ("padding-inline-end", "paddingRight"),
    ("padding-block-start", "paddingTop"),
    ("padding-block-end", "paddingBottom"),
    ("inset-inline-start", "left"),
    ("inset-inline-end", "right"),
    ("inset-block-start", "top"),
    ("inset-block-end", "bottom"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prop(u8);

const fn same(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

impl Prop {
    /// The property with the style-object name `name`; a typo fails the build.
    pub const fn of(name: &str) -> Self {
        let mut index = 0;
        while index < PROPERTIES.len() {
            if same(PROPERTIES[index].name, name) {
                return Self(index as u8);
            }
            index += 1;
        }
        panic!("unknown property");
    }
    /// The property with the style-object name `name`, rejected ones included.
    pub fn named(name: &str) -> Option<Self> {
        let index = PROPERTIES
            .iter()
            .position(|definition| definition.name == name)?;
        Some(Self(index as u8))
    }
    pub fn name(self) -> &'static str {
        PROPERTIES[usize::from(self.0)].name
    }
    pub fn inherited(self) -> bool {
        PROPERTIES[usize::from(self.0)].inherited
    }
    pub fn unsupported(self) -> Option<&'static str> {
        PROPERTIES[usize::from(self.0)].unsupported
    }
}

fn css_name(name: &str) -> Option<Prop> {
    if let Some(index) = PROPERTIES
        .iter()
        .position(|definition| definition.css == name)
    {
        return Some(Prop(index as u8));
    }
    let (_, target) = ALIASES.iter().find(|(alias, _)| *alias == name)?;
    Some(Prop::of(target))
}

/// The key a stylesheet or inline-style declaration of `name` sets. `context` ends each message.
pub fn property(name: &str, context: &str) -> Result<Key, Message> {
    if name.starts_with("--") {
        return Ok(Key::Custom(name.into()));
    }
    // CSS property names are ASCII case-insensitive; camelCase is the JSX spelling, not CSS.
    let Some(prop) = css_name(&name.to_lowercase()) else {
        let mut kebab = String::with_capacity(name.len() + 4);
        for character in name.chars() {
            if character.is_ascii_uppercase() {
                kebab.push('-');
                kebab.push(character.to_ascii_lowercase());
            } else {
                kebab.push(character);
            }
        }
        let hint = match css_name(&kebab) {
            Some(_) => format!(" (use kebab-case: {kebab})"),
            None => String::new(),
        };
        return Err(format!("unsupported CSS property: {name}{hint}{context}").into());
    };
    match prop.unsupported() {
        Some(reason) => Err(format!("unsupported CSS property: {name} ({reason}){context}").into()),
        None => Ok(Key::Prop(prop)),
    }
}
