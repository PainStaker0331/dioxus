/*
This example is a simple iOS-style calculator. This particular example can run any platform - Web, Mobile, Desktop.
This calculator version uses React-style state management. All state is held as individual use_states.
*/

use std::sync::Arc;

use dioxus::events::*;
use dioxus::prelude::*;
use separator::Separatable;

fn main() {
    dioxus::desktop::launch(APP, |cfg| cfg);
}

const APP: FC<()> = |cx, _| {
    let cur_val = use_state(cx, || 0.0_f64);
    let operator = use_state(cx, || None as Option<&'static str>);
    let display_value = use_state(cx, || String::from(""));

    let toggle_percent = move |_| todo!();
    let input_digit = move |num: u8| display_value.modify().push_str(num.to_string().as_str());

    rsx!(cx, div {
        class: "calculator",
        onkeydown: move |evt| match evt.key_code {
            KeyCode::Add => operator.set(Some("+")),
            KeyCode::Subtract => operator.set(Some("-")),
            KeyCode::Divide => operator.set(Some("/")),
            KeyCode::Multiply => operator.set(Some("*")),
            KeyCode::Num0 => input_digit(0),
            KeyCode::Num1 => input_digit(1),
            KeyCode::Num2 => input_digit(2),
            KeyCode::Num3 => input_digit(3),
            KeyCode::Num4 => input_digit(4),
            KeyCode::Num5 => input_digit(5),
            KeyCode::Num6 => input_digit(6),
            KeyCode::Num7 => input_digit(7),
            KeyCode::Num8 => input_digit(8),
            KeyCode::Num9 => input_digit(9),
            KeyCode::Backspace => {
                if !display_value.as_str().eq("0") {
                    display_value.modify().pop();
                }
            }
            _ => {}
        }
        div { class: "calculator-display", {[format_args!("{}", cur_val.separated_string())]} }
        div { class: "input-keys"
            div { class: "function-keys"
                CalculatorKey {
                    {[if display_value == "0" { "C" } else { "AC" }]}
                    name: "key-clear",
                    onclick: move |_| {
                        display_value.set("0".to_string());
                        if display_value != "0" {
                            operator.set(None);
                            cur_val.set(0.0);
                        }
                    }
                }
                CalculatorKey {
                    "±"
                    name: "key-sign",
                    onclick: move |_| {
                        if display_value.starts_with("-") {
                            display_value.set(display_value.trim_start_matches("-").to_string())
                        } else {
                            display_value.set(format!("-{}", *display_value))
                        }
                    },
                }
                CalculatorKey {
                    "%"
                    onclick: {toggle_percent}
                    name: "key-percent",
                }
            }
            div { class: "digit-keys"
                CalculatorKey { name: "key-0", onclick: move |_| input_digit(0), "0" }
                CalculatorKey { name: "key-dot", onclick: move |_| display_value.modify().push_str("."), "●" }

                {(1..9).map(|k| rsx!{
                    CalculatorKey { key: "{k}", name: "key-{k}", onclick: move |_| input_digit(k), "{k}" }
                })}
            }
            div { class: "operator-keys"
                CalculatorKey { name: "key-divide", onclick: move |_| operator.set(Some("/")) "÷" }
                CalculatorKey { name: "key-multiply", onclick: move |_| operator.set(Some("*")) "×" }
                CalculatorKey { name: "key-subtract", onclick: move |_| operator.set(Some("-")) "−" }
                CalculatorKey { name: "key-add", onclick: move |_| operator.set(Some("+")) "+" }
                CalculatorKey {
                    "="
                    name: "key-equals",
                    onclick: move |_| {
                        if let Some(op) = operator.as_ref() {
                            let rhs = display_value.parse::<f64>().unwrap();
                            let new_val = match *op {
                                "+" => *cur_val + rhs,
                                "-" => *cur_val - rhs,
                                "*" => *cur_val * rhs,
                                "/" => *cur_val / rhs,
                                _ => unreachable!(),
                            };
                            cur_val.set(new_val);
                            display_value.set(new_val.to_string());
                            operator.set(None);
                        }
                    },
                }
            }
        }
    })
};

#[derive(Props)]
struct CalculatorKeyProps<'a> {
    name: &'static str,
    onclick: &'a dyn Fn(Arc<MouseEvent>),
    children: Element,
}

fn CalculatorKey<'a>(cx: Context, props: &CalculatorKeyProps) -> Element {
    rsx!(cx, button {
        class: "calculator-key {props.name}"
        onclick: {props.onclick}
        {&props.children}
    })
}
