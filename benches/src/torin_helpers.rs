use rand::distr::uniform::SampleRange;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use taffy::prelude::{
    AlignItems, Dimension, FlexDirection, JustifyContent, LengthPercentage, LengthPercentageAuto, Style,
};
use taffy::style::{
    AlignContentKeyword, AlignItemsKeyword, ExpandedDimension, ExpandedLengthPercentage, ExpandedLengthPercentageAuto,
    Position,
};
use torin::prelude::{
    Alignment, Area, Content, Direction, Gaps, Node, NoopMeasurer, Position as TorinPosition, Size, Torin, TreeAdapter,
};

use crate::{BuildTree, BuildTreeExt, GenStyle};

#[derive(Default)]
pub struct Fixture {
    nodes: Vec<FixtureNode>,
}

struct FixtureNode {
    parent: Option<usize>,
    children: Vec<usize>,
    depth: u16,
    node: Node,
}

impl Fixture {
    fn push(&mut self, node: Node, children: &[usize]) -> usize {
        let id = self.nodes.len();
        self.nodes.push(FixtureNode { parent: None, children: children.to_vec(), depth: 0, node });
        id
    }

    fn attach_root_children(&mut self, children: &[usize]) {
        self.nodes[0].children = children.to_vec();
        self.assign_parent_depth(0, None, 0);
    }

    fn assign_parent_depth(&mut self, id: usize, parent: Option<usize>, depth: u16) {
        self.nodes[id].parent = parent;
        self.nodes[id].depth = depth;
        let children = self.nodes[id].children.clone();
        for child in children {
            self.assign_parent_depth(child, Some(id), depth + 1);
        }
    }
}

impl TreeAdapter<usize> for Fixture {
    fn root_id(&self) -> usize {
        0
    }

    fn read_node<R>(&self, id: &usize, reader: impl FnOnce(&Node, &[usize]) -> R) -> Option<R> {
        self.nodes.get(*id).map(|entry| reader(&entry.node, &entry.children))
    }

    fn height(&self, id: &usize) -> Option<u16> {
        self.nodes.get(*id).map(|entry| entry.depth)
    }

    fn parent_of(&self, id: &usize) -> Option<usize> {
        self.nodes.get(*id).and_then(|entry| entry.parent)
    }
}

pub struct TorinTreeBuilder<R: Rng, G: GenStyle<Style>> {
    rng: R,
    style_generator: G,
    fixture: Fixture,
    layout: Torin<usize>,
    measurer: Option<NoopMeasurer>,
}

impl<R: Rng, G: GenStyle<Style>> BuildTree<R, G> for TorinTreeBuilder<R, G> {
    const NAME: &'static str = "Torin";
    type Tree = Fixture;
    type Node = usize;

    fn with_rng(mut rng: R, mut style_generator: G) -> Self {
        let root = style_generator.create_root_style(&mut rng);
        let mut fixture = Fixture::default();
        fixture.push(to_torin_node(&root), &[]);
        Self { rng, style_generator, fixture, layout: Torin::new(), measurer: Some(NoopMeasurer) }
    }

    fn compute_layout_inner(&mut self, available_width: Option<f32>, available_height: Option<f32>) {
        let root_area = Area::from_size((available_width.unwrap_or(1000.), available_height.unwrap_or(1000.)).into());
        self.layout.measure(0, root_area, &mut self.measurer, &self.fixture);
    }

    fn mark_all_dirty(&mut self) {
        self.layout.invalidate(0);
        self.layout.find_best_root(&self.fixture);
    }

    fn random_usize(&mut self, range: impl SampleRange<usize>) -> usize {
        self.rng.random_range(range)
    }

    fn create_leaf_node(&mut self) -> Self::Node {
        let style = self.style_generator.create_leaf_style(&mut self.rng);
        self.fixture.push(to_torin_node(&style), &[])
    }

    fn create_container_node(&mut self, children: &[Self::Node]) -> Self::Node {
        let style = self.style_generator.create_container_style(&mut self.rng);
        self.fixture.push(to_torin_node(&style), children)
    }

    fn create_node_with_style(&mut self, style: Style, children: &[Self::Node]) -> Self::Node {
        self.fixture.push(to_torin_node(&style), children)
    }

    fn set_root_children(&mut self, children: &[Self::Node]) {
        self.fixture.attach_root_children(children);
    }

    fn total_node_count(&mut self) -> usize {
        self.fixture.nodes.len()
    }

    fn into_tree_and_root(self) -> (Self::Tree, Self::Node) {
        (self.fixture, 0)
    }
}

impl<G: GenStyle<Style>> BuildTreeExt<G> for TorinTreeBuilder<ChaCha8Rng, G> {}

fn to_torin_node(style: &Style) -> Node {
    let width = to_torin_size(style.size.width);
    let height = to_torin_size(style.size.height);
    let (width, height) = if style.flex_grow > 0. {
        match style.flex_direction {
            FlexDirection::Row => (Size::fill(), height),
            FlexDirection::Column => (width, Size::fill()),
            FlexDirection::RowReverse | FlexDirection::ColumnReverse => (width, Size::fill()),
        }
    } else {
        (width, height)
    };
    let mut node = Node {
        width,
        height,
        direction: match style.flex_direction {
            FlexDirection::Row | FlexDirection::RowReverse => Direction::Horizontal,
            FlexDirection::Column | FlexDirection::ColumnReverse => Direction::Vertical,
        },
        content: Content::Flex,
        main_alignment: to_torin_main_alignment(style.justify_content),
        cross_alignment: to_torin_cross_alignment(style.align_items),
        padding: to_torin_padding(style.padding),
        margin: to_torin_margin(style.margin),
        ..Node::default()
    };

    if style.position == Position::Absolute || style.position == Position::Fixed {
        let mut position = TorinPosition::new_absolute();
        let inset = style.inset;
        if let Some(value) = to_torin_offset(inset.top) {
            position = position.top(value);
        }
        if let Some(value) = to_torin_offset(inset.right) {
            position = position.right(value);
        }
        if let Some(value) = to_torin_offset(inset.bottom) {
            position = position.bottom(value);
        }
        if let Some(value) = to_torin_offset(inset.left) {
            position = position.left(value);
        }
        node.position = position;
    }

    node
}

fn to_torin_size(value: Dimension) -> Size {
    match value.expand() {
        ExpandedDimension::Length(value) => Size::px(value),
        ExpandedDimension::Percent(value) => Size::percent(value * 100.),
        ExpandedDimension::Auto => Size::Inner,
        ExpandedDimension::Stretch => Size::fill(),
        _ => Size::Inner,
    }
}

fn to_torin_offset(value: LengthPercentageAuto) -> Option<f32> {
    match value.expand() {
        ExpandedLengthPercentageAuto::Length(value) => Some(value),
        ExpandedLengthPercentageAuto::Percent(value) => Some(value * 1000.),
        ExpandedLengthPercentageAuto::Auto => None,
        _ => None,
    }
}

fn to_torin_margin(value: taffy::prelude::Rect<LengthPercentageAuto>) -> Gaps {
    fn gap(value: LengthPercentageAuto) -> f32 {
        match value.expand() {
            ExpandedLengthPercentageAuto::Length(value) => value,
            ExpandedLengthPercentageAuto::Percent(value) => value * 1000.,
            _ => 0.,
        }
    }

    Gaps::new(gap(value.top), gap(value.right), gap(value.bottom), gap(value.left))
}

fn to_torin_padding(value: taffy::prelude::Rect<LengthPercentage>) -> Gaps {
    fn gap(value: LengthPercentage) -> f32 {
        match value.expand() {
            ExpandedLengthPercentage::Length(value) => value,
            ExpandedLengthPercentage::Percent(value) => value * 1000.,
            _ => 0.,
        }
    }

    Gaps::new(gap(value.top), gap(value.right), gap(value.bottom), gap(value.left))
}

fn to_torin_main_alignment(value: JustifyContent) -> Alignment {
    match value.keyword() {
        AlignContentKeyword::Center => Alignment::Center,
        AlignContentKeyword::End | AlignContentKeyword::FlexEnd => Alignment::End,
        _ => Alignment::Start,
    }
}

fn to_torin_cross_alignment(value: AlignItems) -> Alignment {
    match value.keyword() {
        AlignItemsKeyword::Center => Alignment::Center,
        AlignItemsKeyword::End | AlignItemsKeyword::FlexEnd => Alignment::End,
        _ => Alignment::Start,
    }
}
