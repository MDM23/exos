# Binding to the DOM

Attribute blocks produce attributes, one value at a time, repeated as needed.

| block | emits |
| --- | --- |
| `{&handle}` | declares signals on this element's scope |
| `{on_click(...)}`, `{on(EventType::PointerDown, ...)}` | a delegated handler |
| `{text(expression)}` | text content |
| `{show(expression)}` | toggles `hidden` |
| `{class(name, expression)}` | one class toggle |
| `{attr(name, expression)}` | one attribute |
| `{prop(name, expression)}` | one property (`value`, `checked`, ...) |
| `{bind(&signal)}` | two-way binding for a form control |
| `{preserve()}` | never morph this element |

Raw `data-*` attributes still work, so none of this is a wall. The sortable
plugin is reached that way, since it is opt-in JavaScript rather than API
surface:

```rust
view! {
    <ul id="file-list" data-sortable="post('/files/reorder', { order: $._order })">
        <li data-sort-item={ entry.id }>
            <span data-drag-handle>"::"</span>
        </li>
    </ul>
}
```
