use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event as TermEvent, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dioxus_core::exports::futures_channel::mpsc::unbounded;
use dioxus_core::*;
use dioxus_native_core::real_dom::RealDom;
use futures::{
    channel::mpsc::{UnboundedReceiver, UnboundedSender},
    pin_mut, StreamExt,
};
use layout::StretchLayout;
use std::{io, time::Duration};
use stretch2::{prelude::Size, Stretch};
use style_attributes::StyleModifier;
use tui::{backend::CrosstermBackend, Terminal};

mod config;
mod hooks;
mod layout;
mod render;
mod style;
mod style_attributes;
mod widget;

pub use config::*;
pub use hooks::*;
pub use render::*;

#[derive(Clone)]
pub struct TuiContext {
    tx: UnboundedSender<InputEvent>,
}
impl TuiContext {
    pub fn quit(&self) {
        self.tx.unbounded_send(InputEvent::Close).unwrap();
    }
}

pub fn launch(app: Component<()>) {
    launch_cfg(app, Config::default())
}

pub fn launch_cfg(app: Component<()>, cfg: Config) {
    let mut dom = VirtualDom::new(app);

    let (handler, state, register_event) = RinkInputHandler::new();

    // Setup input handling
    let (event_tx, event_rx) = unbounded();
    let event_tx_clone = event_tx.clone();
    std::thread::spawn(move || {
        let tick_rate = Duration::from_millis(1000);
        loop {
            if crossterm::event::poll(tick_rate).unwrap() {
                // if crossterm::event::poll(timeout).unwrap() {
                let evt = crossterm::event::read().unwrap();
                if event_tx.unbounded_send(InputEvent::UserInput(evt)).is_err() {
                    break;
                }
            }
        }
    });

    let cx = dom.base_scope();
    cx.provide_root_context(state);
    cx.provide_root_context(TuiContext { tx: event_tx_clone });

    let mut rdom: RealDom<StretchLayout, StyleModifier> = RealDom::new();
    let mutations = dom.rebuild();
    let to_update = rdom.apply_mutations(vec![mutations]);
    let mut stretch = Stretch::new();
    let _to_rerender = rdom
        .update_state(&dom, to_update, &mut stretch, &mut ())
        .unwrap();

    render_vdom(
        &mut dom,
        event_rx,
        handler,
        cfg,
        rdom,
        stretch,
        register_event,
    )
    .unwrap();
}

fn render_vdom(
    vdom: &mut VirtualDom,
    mut event_reciever: UnboundedReceiver<InputEvent>,
    handler: RinkInputHandler,
    cfg: Config,
    mut rdom: RealDom<StretchLayout, StyleModifier>,
    mut stretch: Stretch,
    mut register_event: impl FnMut(crossterm::event::Event),
) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            enable_raw_mode().unwrap();
            let mut stdout = std::io::stdout();
            execute!(stdout, EnterAlternateScreen, EnableMouseCapture).unwrap();
            let backend = CrosstermBackend::new(io::stdout());
            let mut terminal = Terminal::new(backend).unwrap();

            terminal.clear().unwrap();
            let mut to_rerender: fxhash::FxHashSet<usize> = vec![0].into_iter().collect();
            let mut resized = true;

            loop {
                /*
                -> render the nodes in the right place with tui/crossterm
                -> wait for changes
                -> resolve events
                -> lazily update the layout and style based on nodes changed

                use simd to compare lines for diffing?

                todo: lazy re-rendering
                */

                if !to_rerender.is_empty() || resized {
                    resized = false;
                    terminal.draw(|frame| {
                        // size is guaranteed to not change when rendering
                        let dims = frame.size();
                        let width = dims.width;
                        let height = dims.height;
                        let root_id = rdom.root_id();
                        let root_node = rdom[root_id].up_state.node.unwrap();

                        stretch
                            .compute_layout(
                                root_node,
                                Size {
                                    width: stretch2::prelude::Number::Defined((width - 1) as f32),
                                    height: stretch2::prelude::Number::Defined((height - 1) as f32),
                                },
                            )
                            .unwrap();
                        let root = &rdom[rdom.root_id()];
                        render::render_vnode(frame, &stretch, &rdom, &root, cfg);
                    })?;
                }

                use futures::future::{select, Either};
                {
                    let wait = vdom.wait_for_work();
                    pin_mut!(wait);

                    match select(wait, event_reciever.next()).await {
                        Either::Left((_a, _b)) => {
                            //
                        }
                        Either::Right((evt, _o)) => {
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
                                    TermEvent::Resize(_, _) => resized = true,
                                    TermEvent::Mouse(_) => {}
                                },
                                InputEvent::Close => break,
                            };

                            if let InputEvent::UserInput(evt) = evt.unwrap() {
                                register_event(evt);
                            }
                        }
                    }
                }

                {
                    // resolve events before rendering
                    let evts = handler.get_events(&stretch, &mut rdom);
                    for e in evts {
                        vdom.handle_message(SchedulerMsg::Event(e));
                    }
                    let mutations = vdom.work_with_deadline(|| false);
                    // updates the dom's nodes
                    let to_update = rdom.apply_mutations(mutations);
                    // update the style and layout
                    to_rerender.extend(
                        rdom.update_state(vdom, to_update, &mut stretch, &mut ())
                            .unwrap()
                            .iter(),
                    )
                }
            }

            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;

            Ok(())
        })
}

enum InputEvent {
    UserInput(TermEvent),
    Close,
}
