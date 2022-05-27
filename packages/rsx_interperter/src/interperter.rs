use dioxus_core::{Attribute, NodeFactory, VNode};
use dioxus_rsx::{BodyNode, CallBody, ElementAttr};
use quote::ToTokens;
use std::str::FromStr;

use crate::attributes::attrbute_to_static_str;
use crate::captuered_context::{CapturedContext, IfmtArgs};
use crate::elements::element_to_static_str;

#[derive(Debug)]
enum Segment {
    Ident(String),
    Literal(String),
}

struct InterperedIfmt {
    segments: Vec<Segment>,
}

impl InterperedIfmt {
    fn resolve(&self, captured: &IfmtArgs) -> String {
        let mut result = String::new();
        for seg in &self.segments {
            match seg {
                Segment::Ident(name) => {
                    let (_, value) = captured
                        .named_args
                        .iter()
                        .find(|(n, _)| *n == name)
                        .expect(format!("could not resolve {}", name).as_str());
                    result.push_str(value);
                }
                Segment::Literal(lit) => result.push_str(lit),
            }
        }
        result
    }
}

impl FromStr for InterperedIfmt {
    type Err = ();
    fn from_str(input: &str) -> Result<Self, ()> {
        let mut segments = Vec::new();
        let mut segment = String::new();
        let mut chars = input.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '{' {
                if chars.peek().copied() != Some('{') {
                    let old;
                    (old, segment) = (segment, String::new());
                    if !old.is_empty() {
                        segments.push(Segment::Literal(old));
                    }
                    while let Some(c) = chars.next() {
                        if c == '}' {
                            let old;
                            (old, segment) = (segment, String::new());
                            if !old.is_empty() {
                                segments.push(Segment::Ident(old));
                            }
                            break;
                        }
                        if c == ':' {
                            while Some('}') != chars.next() {}
                            let old;
                            (old, segment) = (segment, String::new());
                            if !old.is_empty() {
                                segments.push(Segment::Ident(old));
                            }
                            break;
                        }
                        segment.push(c);
                    }
                }
            } else {
                segment.push(c);
            }
        }
        if !segment.is_empty() {
            segments.push(Segment::Literal(segment));
        }
        Ok(Self { segments })
    }
}

pub fn build<'a>(rsx: CallBody, ctx: CapturedContext, factory: &NodeFactory<'a>) -> VNode<'a> {
    let children_built = factory.bump().alloc(Vec::new());
    for (i, child) in rsx.roots.into_iter().enumerate() {
        children_built.push(build_node(child, &ctx, factory, i.to_string().as_str()));
    }
    factory.fragment_from_iter(children_built.iter())
}

fn build_node<'a>(
    node: BodyNode,
    ctx: &CapturedContext,
    factory: &NodeFactory<'a>,
    key: &str,
) -> Option<VNode<'a>> {
    let bump = factory.bump();
    match node {
        BodyNode::Text(text) => {
            let ifmt: InterperedIfmt = text.value().parse().unwrap();
            let text = bump.alloc(ifmt.resolve(&ctx.captured));
            Some(factory.text(format_args!("{}", text)))
        }
        BodyNode::Element(el) => {
            let attributes: &mut Vec<Attribute> = bump.alloc(Vec::new());
            for attr in el.attributes {
                match attr.attr {
                    ElementAttr::AttrText { .. } | ElementAttr::CustomAttrText { .. } => {
                        let (name, value): (String, InterperedIfmt) = match attr.attr {
                            ElementAttr::AttrText { name, value } => {
                                (name.to_string(), value.value().parse().unwrap())
                            }
                            ElementAttr::CustomAttrText { name, value } => {
                                (name.value(), value.value().parse().unwrap())
                            }
                            _ => unreachable!(),
                        };

                        if let Some((name, namespace)) = attrbute_to_static_str(&name) {
                            let value = bump.alloc(value.resolve(&ctx.captured));
                            attributes.push(Attribute {
                                name,
                                value,
                                is_static: true,
                                is_volatile: false,
                                namespace,
                            });
                        } else {
                            return None;
                        }
                    }

                    ElementAttr::AttrExpression { .. }
                    | ElementAttr::CustomAttrExpression { .. } => {
                        let (name, value) = match attr.attr {
                            ElementAttr::AttrExpression { name, value } => {
                                (name.to_string(), value)
                            }
                            ElementAttr::CustomAttrExpression { name, value } => {
                                (name.value(), value)
                            }

                            _ => unreachable!(),
                        };
                        let formatted_expr = format!("{}", value.to_token_stream());
                        if let Some((_, resulting_value)) =
                            ctx.expressions.iter().find(|(n, _)| *n == formatted_expr)
                        {
                            if let Some((name, namespace)) = attrbute_to_static_str(&name) {
                                let value = bump.alloc(resulting_value.clone());
                                attributes.push(Attribute {
                                    name,
                                    value,
                                    is_static: true,
                                    is_volatile: false,
                                    namespace,
                                });
                            }
                        } else {
                            panic!("could not resolve expression {}", formatted_expr);
                        }
                    }
                    // Path(ExprPath { attrs: [], qself: None, path: Path { leading_colon: None, segments: [PathSegment { ident: Ident { ident: \"count\", span: #0 bytes(497..502) }, arguments: None }] } })
                    // Path(ExprPath { attrs: [], qself: None, path: Path { leading_colon: None, segments: [PathSegment { ident: Ident(count), arguments: None }] } })
                    _ => (),
                };
            }
            let children = bump.alloc(Vec::new());
            for (i, child) in el.children.into_iter().enumerate() {
                let node = build_node(child, ctx, factory, i.to_string().as_str());
                if let Some(node) = node {
                    children.push(node);
                }
            }
            let tag = bump.alloc(el.name.to_string());
            if let Some((tag, ns)) = element_to_static_str(tag) {
                Some(factory.raw_element(
                    tag,
                    ns,
                    &[],
                    attributes.as_slice(),
                    children.as_slice(),
                    Some(format_args!("{}", key)),
                ))
            } else {
                None
            }
        }
        BodyNode::Component(_) => todo!(),
        BodyNode::RawExpr(_) => todo!(),
        BodyNode::Meta(_) => todo!(),
    }
}
