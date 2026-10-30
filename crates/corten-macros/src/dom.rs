//! What the browser calls things: the DOM interface of each HTML tag and of each event, as
//! TypeScript's `lib.dom` names them. `html!` types `ref={..}`, `target` and `event` with them, and
//! `#[component(js)]` reads them from a handler's JSDoc.

/// `HTMLElementTagNameMap`: the interface of an HTML tag (`button` is `HTMLButtonElement`). SVG
/// tags are `SVGElement` (`svg` itself `SVGSVGElement`); custom elements and the rest are
/// `HTMLElement`.
pub fn element(tag: &str) -> &'static str {
    match tag {
        "a" => "HTMLAnchorElement",
        "area" => "HTMLAreaElement",
        "audio" => "HTMLAudioElement",
        "br" => "HTMLBRElement",
        "base" => "HTMLBaseElement",
        "body" => "HTMLBodyElement",
        "button" => "HTMLButtonElement",
        "canvas" => "HTMLCanvasElement",
        "caption" => "HTMLTableCaptionElement",
        "col" | "colgroup" => "HTMLTableColElement",
        "data" => "HTMLDataElement",
        "datalist" => "HTMLDataListElement",
        "del" | "ins" => "HTMLModElement",
        "details" => "HTMLDetailsElement",
        "dialog" => "HTMLDialogElement",
        "div" => "HTMLDivElement",
        "dl" => "HTMLDListElement",
        "embed" => "HTMLEmbedElement",
        "fieldset" => "HTMLFieldSetElement",
        "form" => "HTMLFormElement",
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "HTMLHeadingElement",
        "head" => "HTMLHeadElement",
        "hr" => "HTMLHRElement",
        "html" => "HTMLHtmlElement",
        "iframe" => "HTMLIFrameElement",
        "img" => "HTMLImageElement",
        "input" => "HTMLInputElement",
        "label" => "HTMLLabelElement",
        "legend" => "HTMLLegendElement",
        "li" => "HTMLLIElement",
        "link" => "HTMLLinkElement",
        "map" => "HTMLMapElement",
        "menu" => "HTMLMenuElement",
        "meta" => "HTMLMetaElement",
        "meter" => "HTMLMeterElement",
        "object" => "HTMLObjectElement",
        "ol" => "HTMLOListElement",
        "optgroup" => "HTMLOptGroupElement",
        "option" => "HTMLOptionElement",
        "output" => "HTMLOutputElement",
        "p" => "HTMLParagraphElement",
        "picture" => "HTMLPictureElement",
        "pre" => "HTMLPreElement",
        "progress" => "HTMLProgressElement",
        "q" | "blockquote" => "HTMLQuoteElement",
        "script" => "HTMLScriptElement",
        "select" => "HTMLSelectElement",
        "slot" => "HTMLSlotElement",
        "source" => "HTMLSourceElement",
        "span" => "HTMLSpanElement",
        "style" => "HTMLStyleElement",
        "table" => "HTMLTableElement",
        "tbody" | "thead" | "tfoot" => "HTMLTableSectionElement",
        "td" | "th" => "HTMLTableCellElement",
        "template" => "HTMLTemplateElement",
        "textarea" => "HTMLTextAreaElement",
        "time" => "HTMLTimeElement",
        "title" => "HTMLTitleElement",
        "tr" => "HTMLTableRowElement",
        "track" => "HTMLTrackElement",
        "ul" => "HTMLUListElement",
        "video" => "HTMLVideoElement",
        "svg" => "SVGSVGElement",
        "path" | "g" | "circle" | "ellipse" | "rect" | "line" | "polyline" | "polygon" | "text" | "tspan" | "use" | "defs"
        | "symbol" | "clipPath" | "mask" | "pattern" | "linearGradient" | "radialGradient" | "stop" | "image"
        | "foreignObject" => "SVGElement",
        _ => "HTMLElement",
    }
}

/// Every element interface `corten::script::el` has a marker for.
pub const ELEMENTS: &[&str] = &[
    "Element",
    "HTMLElement",
    "HTMLMediaElement",
    "SVGElement",
    "SVGSVGElement",
    "HTMLAnchorElement",
    "HTMLAreaElement",
    "HTMLAudioElement",
    "HTMLBRElement",
    "HTMLBaseElement",
    "HTMLBodyElement",
    "HTMLButtonElement",
    "HTMLCanvasElement",
    "HTMLDataElement",
    "HTMLDataListElement",
    "HTMLDetailsElement",
    "HTMLDialogElement",
    "HTMLDivElement",
    "HTMLDListElement",
    "HTMLEmbedElement",
    "HTMLFieldSetElement",
    "HTMLFormElement",
    "HTMLHeadingElement",
    "HTMLHeadElement",
    "HTMLHRElement",
    "HTMLHtmlElement",
    "HTMLIFrameElement",
    "HTMLImageElement",
    "HTMLInputElement",
    "HTMLLabelElement",
    "HTMLLegendElement",
    "HTMLLIElement",
    "HTMLLinkElement",
    "HTMLMapElement",
    "HTMLMenuElement",
    "HTMLMetaElement",
    "HTMLMeterElement",
    "HTMLModElement",
    "HTMLObjectElement",
    "HTMLOListElement",
    "HTMLOptGroupElement",
    "HTMLOptionElement",
    "HTMLOutputElement",
    "HTMLParagraphElement",
    "HTMLPictureElement",
    "HTMLPreElement",
    "HTMLProgressElement",
    "HTMLQuoteElement",
    "HTMLScriptElement",
    "HTMLSelectElement",
    "HTMLSlotElement",
    "HTMLSourceElement",
    "HTMLSpanElement",
    "HTMLStyleElement",
    "HTMLTableCaptionElement",
    "HTMLTableCellElement",
    "HTMLTableColElement",
    "HTMLTableElement",
    "HTMLTableRowElement",
    "HTMLTableSectionElement",
    "HTMLTemplateElement",
    "HTMLTextAreaElement",
    "HTMLTimeElement",
    "HTMLTitleElement",
    "HTMLTrackElement",
    "HTMLUListElement",
    "HTMLVideoElement",
];

/// `HTMLElementEventMap`: the interface of an event (`click` is `PointerEvent`). `connect`, which
/// Corten fires when an element joins the page, and events lib.dom does not list are `Event`.
pub fn event(name: &str) -> &'static str {
    match name {
        "click" | "auxclick" | "contextmenu" | "pointerdown" | "pointerup" | "pointermove" | "pointerover" | "pointerout"
        | "pointerenter" | "pointerleave" | "pointercancel" | "gotpointercapture" | "lostpointercapture" => "PointerEvent",
        "dblclick" | "mousedown" | "mouseup" | "mousemove" | "mouseover" | "mouseout" | "mouseenter" | "mouseleave" => {
            "MouseEvent"
        }
        "keydown" | "keyup" | "keypress" => "KeyboardEvent",
        "beforeinput" => "InputEvent",
        "focus" | "blur" | "focusin" | "focusout" => "FocusEvent",
        "submit" => "SubmitEvent",
        "toggle" | "beforetoggle" => "ToggleEvent",
        "wheel" => "WheelEvent",
        "drag" | "dragstart" | "dragend" | "dragenter" | "dragleave" | "dragover" | "drop" => "DragEvent",
        "touchstart" | "touchend" | "touchmove" | "touchcancel" => "TouchEvent",
        "animationstart" | "animationend" | "animationiteration" | "animationcancel" => "AnimationEvent",
        "transitionstart" | "transitionend" | "transitionrun" | "transitioncancel" => "TransitionEvent",
        "copy" | "cut" | "paste" => "ClipboardEvent",
        "compositionstart" | "compositionupdate" | "compositionend" => "CompositionEvent",
        "error" => "ErrorEvent",
        _ => "Event",
    }
}

/// Whether a browser event of this name bubbles to the document, so a listener there hears it
/// wherever it happens (`on:document:keydown`). `focus`, `blur`, `mouseenter`, `toggle` and
/// the media and loading events stay on their element, and `selectionchange` is not dispatched
/// up the tree; a name not listed here is refused rather than compiled into a handler that never
/// runs.
pub fn reaches_document(name: &str) -> bool {
    matches!(
        name,
        "click"
            | "dblclick"
            | "auxclick"
            | "contextmenu"
            | "mousedown"
            | "mouseup"
            | "mousemove"
            | "mouseover"
            | "mouseout"
            | "pointerdown"
            | "pointerup"
            | "pointermove"
            | "pointerover"
            | "pointerout"
            | "pointercancel"
            | "gotpointercapture"
            | "lostpointercapture"
            | "wheel"
            | "keydown"
            | "keyup"
            | "keypress"
            | "beforeinput"
            | "input"
            | "change"
            | "select"
            | "submit"
            | "reset"
            | "focusin"
            | "focusout"
            | "drag"
            | "dragstart"
            | "dragend"
            | "dragenter"
            | "dragleave"
            | "dragover"
            | "drop"
            | "touchstart"
            | "touchend"
            | "touchmove"
            | "touchcancel"
            | "copy"
            | "cut"
            | "paste"
            | "compositionstart"
            | "compositionupdate"
            | "compositionend"
            | "animationstart"
            | "animationend"
            | "animationiteration"
            | "animationcancel"
            | "transitionstart"
            | "transitionend"
            | "transitionrun"
            | "transitioncancel"
            | "scroll"
            | "scrollend"
            | "visibilitychange"
            | "fullscreenchange"
    )
}

/// Every event interface `corten::script::ev` has a marker for.
pub const EVENTS: &[&str] = &[
    "Event",
    "UIEvent",
    "MouseEvent",
    "PointerEvent",
    "WheelEvent",
    "DragEvent",
    "KeyboardEvent",
    "InputEvent",
    "FocusEvent",
    "CompositionEvent",
    "TouchEvent",
    "SubmitEvent",
    "ToggleEvent",
    "AnimationEvent",
    "TransitionEvent",
    "ClipboardEvent",
    "ErrorEvent",
    "CustomEvent",
];
