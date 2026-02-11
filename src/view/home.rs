use crate::{ALL_DOWN, Parser};
use anyhow::bail;
use gpui::{
    AnyView, AppContext, ClickEvent, Context, Div, Element, Entity, InteractiveElement,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window, div,
};
use gpui_component::{
    IconName, StyledExt, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    v_flex,
};
use tracing::instrument;

pub struct HomeView {
    input_state: Entity<InputState>,
    is_loading: bool,
    view: Option<AnyView>,
    _subscription: Subscription,
}

impl HomeView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| {
            InputState::new(window, cx).placeholder("输入 URL，支持抖音、B 站 1080P 视频免登录下载")
        });
        let _subscription = cx.subscribe_in(&input_state, window, {
            let input_state = input_state.clone();
            move |view, _, ev, window, cx| {
                if let InputEvent::PressEnter { secondary: _ } = ev {
                    let value = input_state.read(cx).value();
                    let _ = view.parse(value, window, cx);
                }
            }
        });
        Self {
            input_state,
            is_loading: false,
            view: None,
            _subscription,
        }
    }
    pub fn handle_click(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input_state.read(cx).value();
        let _ = self.parse(value, window, cx);
    }
    #[instrument(err, skip(self, window, cx), fields(value = %value))]
    pub fn parse(
        &mut self,
        value: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        if self.is_loading {
            bail!("正在解析中")
        }
        self.is_loading = true;
        cx.notify();
        let parsed = ALL_DOWN.parse(&value, window, cx);
        cx.spawn(async move |view, cx| {
            let parsed = parsed.await;
            view.update(cx, |view, cx| {
                view.is_loading = false;
                if let Ok(parsed) = parsed {
                    view.view = parsed;
                }
                cx.notify();
            })
        })
        .detach();
        Ok(())
    }
}

impl Render for HomeView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.view.is_none() {
            self.render_home(cx)
        } else {
            self.render_parsed(cx)
        }
    }
}

impl HomeView {
    fn title() -> Div {
        v_flex().size_full().child(
            TitleBar::new().child(
                h_flex()
                    .w_full()
                    .pr_2()
                    .justify_between()
                    .child("Unidown 下载器"),
            ),
        )
    }

    fn render_home(&mut self, cx: &mut Context<Self>) -> Div {
        Self::title().child(
            v_flex()
                .id("window-body")
                .p_4()
                .flex_1()
                .justify_center()
                .items_center()
                .gap_4()
                .child(
                    div()
                        .child("Unidown 下载器")
                        .font_bold()
                        .text_3xl()
                        .text_center(),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .w_full()
                        .justify_center()
                        .child(Input::new(&self.input_state).cleanable(true).max_w_128())
                        .child(
                            Button::new("submit")
                                .primary()
                                .icon(IconName::Search)
                                .loading(self.is_loading)
                                .label("解析")
                                .compact()
                                .on_click(cx.listener(Self::handle_click)),
                        ),
                ),
        )
    }

    fn render_parsed(&mut self, cx: &mut Context<Self>) -> Div {
        Self::title().child(
            v_flex()
                .id("window-body")
                .flex_1()
                .overflow_hidden()
                .child(
                    h_flex()
                        .pt_4()
                        .px_4()
                        .gap_2()
                        .w_full()
                        .child(Input::new(&self.input_state).cleanable(true))
                        .child(
                            Button::new("submit")
                                .icon(IconName::Search)
                                .loading(self.is_loading)
                                .label("解析")
                                .compact()
                                .on_click(cx.listener(Self::handle_click)),
                        ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .child(self.view.clone().unwrap().into_any()),
                ),
        )
    }
}
