use crate::focus::Focus;
use anyhow::Result;
use crossterm::{
    cursor::{MoveTo, RestorePosition, SavePosition, Show},
    event::{DisableMouseCapture, EnableMouseCapture, Event as TermEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dioxus_html::EventData;
use dioxus_native_core::{node_ref::NodeMaskBuilder, real_dom::NodeImmutable, Pass, Renderer};
use dioxus_native_core::{real_dom::RealDom, FxDashSet, NodeId, SendAnyMap};
use focus::FocusState;
use futures::{channel::mpsc::UnboundedSender, pin_mut, StreamExt};
use futures_channel::mpsc::unbounded;
use layout::TaffyLayout;
use prevent_default::PreventDefault;
use std::sync::{Arc, Mutex};
use std::{io, time::Duration};
use std::{rc::Rc, sync::RwLock};
use style_attributes::StyleModifier;
use taffy::Taffy;
pub use taffy::{geometry::Point, prelude::*};
use tokio::select;
use tui::{backend::CrosstermBackend, layout::Rect, Terminal};

mod config;
mod focus;
mod hooks;
mod layout;
pub mod prelude;
mod prevent_default;
pub mod query;
mod render;
mod style;
mod style_attributes;
mod widget;
// mod widgets;

pub use config::*;
pub use hooks::*;

// the layout space has a multiplier of 10 to minimize rounding errors
pub(crate) fn screen_to_layout_space(screen: u16) -> f32 {
    screen as f32 * 10.0
}

pub(crate) fn unit_to_layout_space(screen: f32) -> f32 {
    screen * 10.0
}

pub(crate) fn layout_to_screen_space(layout: f32) -> f32 {
    layout / 10.0
}

#[derive(Clone)]
pub struct TuiContext {
    tx: UnboundedSender<InputEvent>,
}
impl TuiContext {
    pub fn quit(&self) {
        self.tx.unbounded_send(InputEvent::Close).unwrap();
    }

    pub fn inject_event(&self, event: crossterm::event::Event) {
        self.tx
            .unbounded_send(InputEvent::UserInput(event))
            .unwrap();
    }
}

pub fn render<R: Renderer<Rc<EventData>>>(
    cfg: Config,
    f: impl FnOnce(&Arc<RwLock<RealDom>>, &Arc<Mutex<Taffy>>, UnboundedSender<InputEvent>) -> R,
) -> Result<()> {
    let mut rdom = RealDom::new(Box::new([
        TaffyLayout::to_type_erased(),
        Focus::to_type_erased(),
        StyleModifier::to_type_erased(),
        PreventDefault::to_type_erased(),
    ]));

    let (handler, state, mut register_event) = RinkInputHandler::craete(&mut rdom);

    // Setup input handling
    let (event_tx, mut event_reciever) = unbounded();
    let event_tx_clone = event_tx.clone();
    if !cfg.headless {
        std::thread::spawn(move || {
            let tick_rate = Duration::from_millis(1000);
            loop {
                if crossterm::event::poll(tick_rate).unwrap() {
                    let evt = crossterm::event::read().unwrap();
                    if event_tx.unbounded_send(InputEvent::UserInput(evt)).is_err() {
                        break;
                    }
                }
            }
        });
    }

    let rdom = Arc::new(RwLock::new(rdom));
    let taffy = Arc::new(Mutex::new(Taffy::new()));
    let mut renderer = f(&rdom, &taffy, event_tx_clone);

    {
        let mut rdom = rdom.write().unwrap();
        let root_id = rdom.root_id();
        renderer.render(rdom.get_mut(root_id).unwrap());
        let mut any_map = SendAnyMap::new();
        any_map.insert(taffy.clone());
        let _ = rdom.update_state(any_map, false);
    }

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            #[cfg(all(feature = "hot-reload", debug_assertions))]
            let mut hot_reload_rx = {
                let (hot_reload_tx, hot_reload_rx) =
                    tokio::sync::mpsc::unbounded_channel::<dioxus_hot_reload::HotReloadMsg>();
                dioxus_hot_reload::connect(move |msg| {
                    let _ = hot_reload_tx.send(msg);
                });
                hot_reload_rx
            };
            let mut terminal = (!cfg.headless).then(|| {
                enable_raw_mode().unwrap();
                let mut stdout = std::io::stdout();
                execute!(
                    stdout,
                    EnterAlternateScreen,
                    EnableMouseCapture,
                    MoveTo(0, 1000)
                )
                .unwrap();
                let backend = CrosstermBackend::new(io::stdout());
                Terminal::new(backend).unwrap()
            });
            if let Some(terminal) = &mut terminal {
                terminal.clear().unwrap();
            }

            let mut to_rerender = FxDashSet::default();
            to_rerender.insert(NodeId(0));
            let mut updated = true;

            loop {
                /*
                -> render the nodes in the right place with tui/crossterm
                -> wait for changes
                -> resolve events
                -> lazily update the layout and style based on nodes changed
                use simd to compare lines for diffing?
                todo: lazy re-rendering
                */

                if !to_rerender.is_empty() || updated {
                    updated = false;
                    fn resize(dims: Rect, taffy: &mut Taffy, rdom: &RealDom) {
                        let width = screen_to_layout_space(dims.width);
                        let height = screen_to_layout_space(dims.height);
                        let root_node = rdom
                            .get(NodeId(0))
                            .unwrap()
                            .get::<TaffyLayout>()
                            .unwrap()
                            .node
                            .unwrap();

                        // the root node fills the entire area

                        let mut style = *taffy.style(root_node).unwrap();
                        style.size = Size {
                            width: Dimension::Points(width),
                            height: Dimension::Points(height),
                        };
                        taffy.set_style(root_node, style).unwrap();

                        let size = Size {
                            width: AvailableSpace::Definite(width),
                            height: AvailableSpace::Definite(height),
                        };
                        taffy.compute_layout(root_node, size).unwrap();
                    }
                    if let Some(terminal) = &mut terminal {
                        execute!(terminal.backend_mut(), SavePosition).unwrap();
                        terminal.draw(|frame| {
                            let rdom = rdom.write().unwrap();
                            let mut taffy = taffy.lock().expect("taffy lock poisoned");
                            // size is guaranteed to not change when rendering
                            resize(frame.size(), &mut taffy, &rdom);
                            let root = rdom.get(rdom.root_id()).unwrap();
                            render::render_vnode(frame, &taffy, root, cfg, Point::ZERO);
                        })?;
                        execute!(terminal.backend_mut(), RestorePosition, Show).unwrap();
                    } else {
                        let rdom = rdom.write().unwrap();
                        resize(
                            Rect {
                                x: 0,
                                y: 0,
                                width: 1000,
                                height: 1000,
                            },
                            &mut taffy.lock().expect("taffy lock poisoned"),
                            &rdom,
                        );
                    }
                }

                // let mut hot_reload_msg = None;
                {
                    let wait = renderer.poll_async();
                    // #[cfg(all(feature = "hot-reload", debug_assertions))]
                    // let hot_reload_wait = hot_reload_rx.recv();
                    // #[cfg(not(all(feature = "hot-reload", debug_assertions)))]
                    // let hot_reload_wait: std::future::Pending<Option<()>> = std::future::pending();

                    pin_mut!(wait);

                    select! {
                        _ = wait => {

                        },
                        evt = event_reciever.next() => {
                            match evt.as_ref().unwrap() {
                                InputEvent::UserInput(event) => match event {
                                    TermEvent::Key(key) => {
                                        if matches!(key.code, KeyCode::Char('C' | 'c'))
                                            && key.modifiers.contains(KeyModifiers::CONTROL)
                                            && cfg.ctrl_c_quit
                                        {
                                            break;
                                        }
                                    }
                                    TermEvent::Resize(_, _) => updated = true,
                                    TermEvent::Mouse(_) => {}
                                },
                                InputEvent::Close => break,
                            };

                            if let InputEvent::UserInput(evt) = evt.unwrap() {
                                register_event(evt);
                            }
                        },
                        // Some(msg) = hot_reload_wait => {
                        //     #[cfg(all(feature = "hot-reload", debug_assertions))]
                        //     {
                        //         hot_reload_msg = Some(msg);
                        //     }
                        //     #[cfg(not(all(feature = "hot-reload", debug_assertions)))]
                        //     let () = msg;
                        // }
                    }
                }

                // // if we have a new template, replace the old one
                // #[cfg(all(feature = "hot-reload", debug_assertions))]
                // if let Some(msg) = hot_reload_msg {
                //     match msg {
                //         dioxus_hot_reload::HotReloadMsg::UpdateTemplate(template) => {
                //             vdom.replace_template(template);
                //         }
                //         dioxus_hot_reload::HotReloadMsg::Shutdown => {
                //             break;
                //         }
                //     }
                // }

                {
                    {
                        let mut rdom = rdom.write().unwrap();
                        let evts = handler
                            .get_events(&taffy.lock().expect("taffy lock poisoned"), &mut rdom);
                        updated |= handler.state().focus_state.clean();

                        for e in evts {
                            let node = rdom.get_mut(e.id).unwrap();
                            renderer.handle_event(node, e.name, e.data, e.bubbles);
                        }
                    }
                    let mut rdom = rdom.write().unwrap();
                    // updates the dom's nodes
                    let root_id = rdom.root_id();
                    renderer.render(rdom.get_mut(root_id).unwrap());
                    // update the style and layout
                    let mut any_map = SendAnyMap::new();
                    any_map.insert(taffy.clone());
                    let (new_to_rerender, dirty) = rdom.update_state(any_map, false);
                    to_rerender = new_to_rerender;
                    let text_mask = NodeMaskBuilder::new().with_text().build();
                    for (id, mask) in dirty {
                        if mask.overlaps(&text_mask) {
                            to_rerender.insert(id);
                        }
                    }
                }
            }

            if let Some(terminal) = &mut terminal {
                disable_raw_mode()?;
                execute!(
                    terminal.backend_mut(),
                    LeaveAlternateScreen,
                    DisableMouseCapture
                )?;
                terminal.show_cursor()?;
            }

            Ok(())
        })
}

#[derive(Debug)]
pub enum InputEvent {
    UserInput(TermEvent),
    Close,
}
