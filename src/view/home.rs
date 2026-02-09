use crate::{ALL_DOWN, AssetGroup, Down, ResourceNode, TOKIO_RT, hash::hash};
use color_eyre::eyre::{bail, eyre};
use gpui::{
    AppContext, ClickEvent, Context, Div, ElementId, Entity, InteractiveElement, IntoElement,
    ParentElement, PathPromptOptions, Render, SharedString, Styled, Subscription, Window, div,
    prelude::FluentBuilder,
};
use gpui_component::{
    IconName, Sizable, StyledExt, TitleBar,
    accordion::Accordion,
    button::{Button, ButtonVariants},
    group_box::GroupBox,
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    v_flex,
};
use tracing::{info, instrument, warn};

pub struct HomeView {
    input_state: Entity<InputState>,
    is_loading: bool,
    parsed: Vec<ResourceNode>,
    _subscription: Subscription,
}

impl HomeView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input_state = cx.new(|cx| InputState::new(window, cx).placeholder("输入 URL"));
        let _subscription = cx.subscribe_in(&input_state, window, {
            let input_state = input_state.clone();
            move |view, _, ev, _, cx| {
                if let InputEvent::PressEnter { secondary: _ } = ev {
                    let value = input_state.read(cx).value();
                    let _ = view.parse(value, cx);
                }
            }
        });
        Self {
            input_state,
            is_loading: false,
            parsed: vec![],
            _subscription,
        }
    }
    pub fn handle_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let value = self.input_state.read(cx).value();
        let _ = self.parse(value, cx);
    }
    #[instrument(err, skip(self, cx), fields(value = %value))]
    pub fn parse(&mut self, value: SharedString, cx: &mut Context<Self>) -> color_eyre::Result<()> {
        if self.is_loading {
            bail!("正在解析中")
        }
        self.is_loading = true;
        cx.notify();
        cx.spawn(async move |view, cx| {
            let parsed = TOKIO_RT
                .spawn(async move { ALL_DOWN.parse(&value).await })
                .await;
            info!("{:#?}", parsed);
            view.update(cx, |view, cx| {
                view.is_loading = false;
                if let Ok(Ok(parsed)) = parsed {
                    view.parsed = parsed;
                }
                cx.notify();
            })
            .map_err(|e| eyre!(e))?;
            Ok::<_, color_eyre::Report>(())
        })
        .detach();
        Ok(())
    }
    pub fn handle_download(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.is_loading = true;
        cx.notify();
        let parsed = self.parsed.clone();
        let options = PathPromptOptions {
            directories: true,
            files: false,
            multiple: false,
            prompt: None,
        };
        let paths = cx.prompt_for_paths(options);
        cx.spawn(async move |view, cx| {
            let mut paths = paths.await;
            if let Ok(Ok(Some(ref mut paths))) = paths
                && let Some(folder_path) = paths.pop()
            {
                info!("用户选择的保存文件夹是: {:?}", folder_path);
                let _ = TOKIO_RT
                    .spawn(async move { ALL_DOWN.download(&parsed, &folder_path).await })
                    .await;
                info!("下载成功");
                view.update(cx, |view, cx| {
                    view.is_loading = false;
                    cx.notify();
                })
                .map_err(|e| eyre!(e))?;
            } else {
                warn!(paths = ?paths, "用户取消了选择或发生了错误" );
                view.update(cx, |view, cx| {
                    view.is_loading = false;
                    cx.notify();
                })
                .map_err(|e| eyre!(e))?;
            }
            Ok::<_, color_eyre::Report>(())
        })
        .detach();
    }
}

impl Render for HomeView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.parsed.is_empty() {
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
                .gap_2()
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
                        )
                        .child(
                            Button::new("download")
                                .primary()
                                .icon(IconName::ArrowDown)
                                .loading(self.is_loading)
                                .label("下载")
                                .compact()
                                .on_click(cx.listener(Self::handle_download)),
                        ),
                )
                .child(
                    v_flex().flex_1().min_h_0().overflow_y_scrollbar().child(
                        div().px_4().pb_4().pt_2().child(Self::render_nodes(
                            &self.parsed,
                            vec![],
                            cx,
                        )),
                    ),
                ),
        )
    }

    fn render_nodes(
        nodes: &[ResourceNode],
        mut path: Vec<usize>,
        cx: &mut Context<Self>,
    ) -> Accordion {
        let mut root = Accordion::new(ElementId::NamedInteger("nodes".into(), hash(&path)))
            .multiple(true)
            .disabled(true);
        path.push(0);
        for (node_idx, node) in nodes.iter().enumerate() {
            *path.last_mut().unwrap() = node_idx;
            let curr_path = path.clone();
            root = root.item(|item| {
                item.open(true)
                    .title(node.title.clone())
                    .child(Self::render_node_content(node, curr_path, cx))
            });
        }
        root
    }

    /// 提取节点主体内容渲染：负责区分渲染 资源组 或 子节点
    fn render_node_content(node: &ResourceNode, path: Vec<usize>, cx: &mut Context<Self>) -> Div {
        let has_groups = !node.asset_groups.is_empty();
        let has_children = !node.children.is_empty();
        div()
            .p_2()
            .gap_2()
            .when(has_groups, |this| {
                this.children(
                    node.asset_groups
                        .iter()
                        .enumerate()
                        .map(|(g_idx, g)| Self::render_asset_group(g, g_idx, path.clone(), cx)),
                )
            })
            .when(has_children, |this| {
                this.child(Self::render_nodes(&node.children, path, cx))
            })
            .when(!has_groups && !has_children, |this| {
                this.child(div().child("无详细内容").text_sm())
            })
    }

    /// 提取资源组渲染：负责渲染 GroupBox 和其中的 Buttons
    fn render_asset_group(
        group: &AssetGroup,
        group_idx: usize,
        path: Vec<usize>,
        cx: &mut Context<Self>,
    ) -> GroupBox {
        GroupBox::new()
            .title(group.title.clone())
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .children(group.variants.iter().enumerate().map(|(v_idx, v)| {
                        let click_path = path.clone();
                        Button::new(ElementId::NamedInteger(
                            "variants".into(),
                            hash(&click_path) ^ group_idx as u64 ^ v_idx as u64,
                        ))
                        .small()
                        .when(v.selected, |this| this.primary())
                        .child(v.label.clone())
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.toggle_variant(&click_path, group_idx, v_idx);
                            cx.notify();
                        }))
                    })),
            )
    }

    fn toggle_variant(&mut self, node_path: &[usize], group_idx: usize, variant_idx: usize) {
        if node_path.is_empty() {
            return;
        }
        let mut curr_children = &mut self.parsed;
        for &idx in node_path.iter().take(node_path.len() - 1) {
            if let Some(node) = curr_children.get_mut(idx) {
                curr_children = &mut node.children;
            } else {
                return;
            }
        }
        if let Some(last_idx) = node_path.last()
            && let Some(node) = curr_children.get_mut(*last_idx)
            && let Some(group) = node.asset_groups.get_mut(group_idx)
            && let Some(variant) = group.variants.get_mut(variant_idx)
        {
            variant.selected = !variant.selected;
        }
    }
}
