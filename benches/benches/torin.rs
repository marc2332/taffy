use std::{collections::HashMap, hint::black_box, time::Instant};

use criterion::{criterion_group, criterion_main, Criterion};
use taffy::prelude::{
    AlignItems, AvailableSpace, Dimension, Display, FlexDirection, JustifyContent, NodeId, Size as TaffySize, Style,
    TaffyTree,
};
use taffy_benchmarks::{benchmark_group, iter_timed, TaffyLayoutTree};
use torin::prelude::*;

struct Fixture {
    nodes: HashMap<usize, (Option<usize>, Vec<usize>, u16, Node)>,
}

impl TreeAdapter<usize> for Fixture {
    fn root_id(&self) -> usize {
        0
    }

    fn read_node<R>(&self, id: &usize, reader: impl FnOnce(&Node, &[usize]) -> R) -> Option<R> {
        self.nodes.get(id).map(|(_, children, _, node)| reader(node, children))
    }

    fn height(&self, id: &usize) -> Option<u16> {
        self.nodes.get(id).map(|(_, _, depth, _)| *depth)
    }

    fn parent_of(&self, id: &usize) -> Option<usize> {
        self.nodes.get(id).and_then(|(parent, _, _, _)| *parent)
    }
}

#[derive(Clone, Copy)]
enum Scenario {
    Wide,
    DeepBinary,
    BalancedAligned,
    FlexRow,
}

impl Scenario {
    const ALL: [Self; 4] = [Self::Wide, Self::DeepBinary, Self::BalancedAligned, Self::FlexRow];

    fn name(self) -> &'static str {
        match self {
            Self::Wide => "wide stack",
            Self::DeepBinary => "deep binary stack",
            Self::BalancedAligned => "balanced aligned stack",
            Self::FlexRow => "flex row",
        }
    }

    fn build(self) -> Fixture {
        let mut nodes = HashMap::new();
        let root = self.node(0);
        nodes.insert(0, (None, Vec::new(), 0, root));

        match self {
            Self::Wide => {
                for id in 1..=10_000 {
                    add_child(&mut nodes, 0, id, 1, self.node(1));
                }
            }
            Self::DeepBinary => {
                let mut parents = vec![0];
                let mut next_id = 1;
                for depth in 1..=13 {
                    let mut next = Vec::new();
                    for parent in parents {
                        for _ in 0..2 {
                            let id = next_id;
                            next_id += 1;
                            add_child(&mut nodes, parent, id, depth, self.node(depth));
                            next.push(id);
                        }
                    }
                    parents = next;
                }
            }
            Self::BalancedAligned => {
                let mut parents = vec![0];
                let mut next_id = 1;
                for depth in 1..=8 {
                    let mut next = Vec::new();
                    for parent in parents {
                        for _ in 0..3 {
                            let id = next_id;
                            next_id += 1;
                            add_child(&mut nodes, parent, id, depth, self.node(depth));
                            next.push(id);
                        }
                    }
                    parents = next;
                }
            }
            Self::FlexRow => {
                for id in 1..=20_000 {
                    add_child(&mut nodes, 0, id, 1, self.node(1));
                }
            }
        }

        Fixture { nodes }
    }

    fn node(self, depth: u16) -> Node {
        let mut node =
            Node { width: Size::px(100.), height: Size::px(30.), direction: Direction::Vertical, ..Node::default() };

        if depth == 0 {
            node.width = Size::px(1000.);
            node.height = Size::px(1000.);
            match self {
                Self::Wide | Self::FlexRow => node.direction = Direction::Horizontal,
                Self::DeepBinary => {}
                Self::BalancedAligned => {
                    node.main_alignment = Alignment::Center;
                    node.cross_alignment = Alignment::End;
                }
            }
            if matches!(self, Self::FlexRow) {
                node.content = Content::Flex;
            }
        } else {
            match self {
                Self::Wide => {
                    node.width = Size::px(20.);
                    node.height = Size::px(20.);
                }
                Self::DeepBinary => {}
                Self::BalancedAligned => {
                    node.width = Size::px(20.);
                    node.height = Size::px(20.);
                    node.main_alignment = Alignment::Center;
                    node.cross_alignment = Alignment::End;
                }
                Self::FlexRow => {
                    node.width = Size::fill();
                    node.height = Size::px(20.);
                }
            }
        }

        node
    }
}

fn build_taffy_tree(fixture: &Fixture) -> TaffyLayoutTree {
    let mut tree = TaffyTree::new();
    tree.disable_rounding();

    let mut ids = Vec::with_capacity(fixture.nodes.len());
    for id in 0..fixture.nodes.len() {
        let node = &fixture.nodes[&id].3;
        ids.push(tree.new_leaf(taffy_style(node)).expect("create Taffy node"));
    }

    for id in 0..fixture.nodes.len() {
        let children: Vec<NodeId> = fixture.nodes[&id].1.iter().map(|child| ids[*child]).collect();
        tree.set_children(ids[id], &children).expect("set Taffy children");
    }

    TaffyLayoutTree::new((tree, ids[0]))
}

fn taffy_style(node: &Node) -> Style {
    fn dimension(size: &Size) -> (Dimension, f32) {
        match size {
            Size::Pixels(value) => (Dimension::length(value.get()), 0.),
            Size::Percentage(value) => (Dimension::percent(value.get() / 100.), 0.),
            Size::Fill => (Dimension::auto(), 1.),
            Size::Inner => (Dimension::auto(), 0.),
            _ => panic!("unsupported benchmark dimension"),
        }
    }

    let (width, flex_grow) = dimension(&node.width);
    let (height, _) = dimension(&node.height);
    Style {
        display: Display::Flex,
        size: TaffySize { width, height },
        flex_direction: match node.direction {
            Direction::Vertical => FlexDirection::Column,
            Direction::Horizontal => FlexDirection::Row,
        },
        flex_grow,
        flex_shrink: 0.,
        align_items: match node.cross_alignment {
            Alignment::Start => AlignItems::START,
            Alignment::Center => AlignItems::CENTER,
            Alignment::End => AlignItems::END,
            _ => panic!("unsupported benchmark cross alignment"),
        },
        justify_content: match node.main_alignment {
            Alignment::Start => JustifyContent::START,
            Alignment::Center => JustifyContent::CENTER,
            Alignment::End => JustifyContent::END,
            _ => panic!("unsupported benchmark main alignment"),
        },
        ..Style::default()
    }
}

fn add_child(
    nodes: &mut HashMap<usize, (Option<usize>, Vec<usize>, u16, Node)>,
    parent: usize,
    id: usize,
    depth: u16,
    node: Node,
) {
    nodes.get_mut(&parent).expect("parent exists").1.push(id);
    nodes.insert(id, (Some(parent), Vec::new(), depth, node));
}

fn torin_benchmarks(c: &mut Criterion) {
    let mut group = benchmark_group(c, "Torin layout from scratch");

    for scenario in Scenario::ALL {
        let fixture = scenario.build();
        let node_count = fixture.nodes.len();
        group.bench_function(format!("Torin / {} ({node_count} nodes)", scenario.name()), |b| {
            let mut layout = Torin::new();
            let mut measurer = Some(NoopMeasurer);
            let root_area = Area::from_size((1000., 1000.).into());

            iter_timed(b, || {
                layout.invalidate(0);
                layout.find_best_root(&fixture);
                let start = Instant::now();
                layout.measure(0, root_area, &mut measurer, &fixture);
                black_box(&layout);
                start.elapsed()
            });
        });

        group.bench_function(format!("Taffy / {} ({node_count} nodes)", scenario.name()), |b| {
            let mut layout = build_taffy_tree(&fixture);

            iter_timed(b, || {
                layout.mark_all_dirty();
                let start = Instant::now();
                layout
                    .tree
                    .compute_layout(
                        layout.root,
                        TaffySize { width: AvailableSpace::Definite(1000.), height: AvailableSpace::Definite(1000.) },
                    )
                    .expect("compute Taffy layout");
                black_box(&layout.tree);
                start.elapsed()
            });
        });
    }

    group.finish();
}

criterion_group!(name = benches; config = taffy_benchmarks::criterion_config(); targets = torin_benchmarks);
criterion_main!(benches);
