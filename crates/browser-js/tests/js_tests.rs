use std::cell::RefCell;
use std::rc::Rc;

use browser_dom::parse_html;
use browser_js::JsEngine;

#[test]
fn js_closures_loops_and_dom_event_mutation() {
    let doc = Rc::new(RefCell::new(parse_html(
        r#"<html><body>
            <p id="count">0</p>
            <button id="inc">Increment</button>
        </body></html>"#,
    )));

    let mut js = JsEngine::new(Rc::clone(&doc));
    js.eval(
        r##"
        let total = 0;
        for (let i = 1; i <= 4; i++) {
            total += i;
        }
        console.log("sum", total);

        const el = document.getElementById("count");
        const btn = document.getElementById("inc");
        let clicks = 0;
        btn.addEventListener("click", () => {
            clicks++;
            el.textContent = "Clicks: " + clicks;
            el.style.color = "#1a73e8";
        });
    "##,
    )
    .unwrap();

    assert_eq!(js.console_logs, vec!["sum 10".to_string()]);

    let btn_id = doc.borrow().get_element_by_id("inc").unwrap();
    js.dispatch_click(btn_id).unwrap();
    js.dispatch_click(btn_id).unwrap();

    let count_id = doc.borrow().get_element_by_id("count").unwrap();
    assert_eq!(doc.borrow().text_content(count_id), "Clicks: 2");
    assert!(
        doc.borrow()
            .get_attribute(count_id, "style")
            .unwrap()
            .contains("color: #1a73e8")
    );
    assert!(js.dom_dirty);
}
