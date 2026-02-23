use crate::Element;

// Helper functions for common elements
pub fn element(tag: &str) -> Element {
    Element::new(tag)
}

pub fn button() -> Element {
    Element::new("button")
}

pub fn div() -> Element {
    Element::new("div")
}

pub fn header() -> Element {
    Element::new("header")
}

pub fn content() -> Element {
    Element::new("content")
}

pub fn footer() -> Element {
    Element::new("footer")
}

pub fn nav() -> Element {
    Element::new("nav")
}

pub fn a() -> Element {
    Element::new("a")
}

pub fn h1() -> Element {
    Element::new("h1")
}

pub fn h2() -> Element {
    Element::new("h2")
}

pub fn h3() -> Element {
    Element::new("h3")
}

pub fn p() -> Element {
    Element::new("p")
}

pub fn span() -> Element {
    Element::new("span")
}

pub fn form() -> Element {
    Element::new("form")
}

pub fn label() -> Element {
    Element::new("label")
}

pub fn ul() -> Element {
    Element::new("ul")
}

pub fn li() -> Element {
    Element::new("li")
}

pub fn i() -> Element {
    Element::new("i")
}

pub fn input() -> Element {
    Element::new("input")
}

pub fn select() -> Element {
    Element::new("select")
}

pub fn option() -> Element {
    Element::new("option")
}

pub fn checkbox() -> Element {
    Element::new("input").attr("type", "checkbox")
}

pub fn meta() -> Element {
    Element::new("meta")
}

pub fn style(content: String) -> Element {
    Element::new("style").text(&content)
}

pub fn script(content: String) -> Element {
    Element::new("script").text(&content)
}
