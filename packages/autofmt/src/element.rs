use crate::{util::*, Buffer};
use dioxus_rsx::*;
use std::{fmt::Result, fmt::Write};

enum ShortOptimization {
    // Special because we want to print the closing bracket immediately
    Empty,

    // Special optimization to put everything on the same line
    Oneliner,

    // Optimization where children flow but props remain fixed on top
    PropsOnTop,

    // The noisiest optimization where everything flows
    NoOpt,
}

impl Buffer {
    pub fn write_element(
        &mut self,
        Element {
            name,
            key,
            attributes,
            children,
            _is_static,
        }: &Element,
        lines: &[&str],
    ) -> Result {
        /*
            1. Write the tag
            2. Write the key
            3. Write the attributes
            4. Write the children
        */

        write!(self.buf, "{name} {{")?;

        // decide if we have any special optimizations
        // Default with none, opt the cases in one-by-one
        let mut opt_level = ShortOptimization::NoOpt;

        // check if we have a lot of attributes
        let is_short_attr_list = is_short_attrs(attributes);
        let is_small_children = is_short_children(children);

        // if we have few attributes and a lot of children, place the attrs on top
        if is_short_attr_list && !is_small_children {
            opt_level = ShortOptimization::PropsOnTop;
        }

        // even if the attr is long, it should be put on one line
        if !is_short_attr_list && attributes.len() <= 1 {
            if children.is_empty() {
                opt_level = ShortOptimization::Oneliner;
            } else {
                opt_level = ShortOptimization::PropsOnTop;
            }
        }

        // if we have few children and few attributes, make it a one-liner
        if is_short_attr_list && is_small_children {
            opt_level = ShortOptimization::Oneliner;
        }

        // If there's nothing at all, empty optimization
        if attributes.is_empty() && children.is_empty() && key.is_none() {
            opt_level = ShortOptimization::Empty;
        }

        match opt_level {
            ShortOptimization::Empty => write!(self.buf, "}}")?,
            ShortOptimization::Oneliner => {
                write!(self.buf, " ")?;
                self.write_attributes(attributes, true)?;

                if !children.is_empty() && !attributes.is_empty() {
                    write!(self.buf, ", ")?;
                }

                // write the children
                for child in children {
                    self.write_ident(lines, child)?;
                }

                write!(self.buf, " }}")?;
            }

            ShortOptimization::PropsOnTop => {
                write!(self.buf, " ")?;
                self.write_attributes(attributes, true)?;

                if !children.is_empty() && !attributes.is_empty() {
                    write!(self.buf, ",")?;
                }

                // write the children
                self.write_body_indented(children, lines)?;

                self.tabbed_line()?;
                write!(self.buf, "}}")?;
            }

            ShortOptimization::NoOpt => {
                // write the key

                // write the attributes
                self.write_attributes(attributes, false)?;

                self.write_body_indented(children, lines)?;

                self.tabbed_line()?;
                write!(self.buf, "}}")?;
            }
        }

        Ok(())
    }

    fn write_attributes(&mut self, attributes: &[ElementAttrNamed], sameline: bool) -> Result {
        let mut attr_iter = attributes.iter().peekable();

        while let Some(attr) = attr_iter.next() {
            if !sameline {
                self.indented_tabbed_line()?;
            }
            self.write_attribute(attr)?;

            if attr_iter.peek().is_some() {
                write!(self.buf, ",")?;

                if sameline {
                    write!(self.buf, " ")?;
                }
            }
        }

        Ok(())
    }

    fn write_attribute(&mut self, attr: &ElementAttrNamed) -> Result {
        match &attr.attr {
            ElementAttr::AttrText { name, value } => {
                write!(self.buf, "{name}: \"{value}\"", value = value.value())?;
            }
            ElementAttr::AttrExpression { name, value } => {
                let out = prettyplease::unparse_expr(value);
                write!(self.buf, "{}: {}", name, out)?;
            }

            ElementAttr::CustomAttrText { name, value } => {
                write!(
                    self.buf,
                    "\"{name}\": \"{value}\"",
                    name = name.value(),
                    value = value.value()
                )?;
            }

            ElementAttr::CustomAttrExpression { name, value } => {
                let out = prettyplease::unparse_expr(value);
                write!(self.buf, "\"{}\": {}", name.value(), out)?;
            }

            ElementAttr::EventTokens { name, tokens } => {
                let out = prettyplease::unparse_expr(tokens);

                let mut lines = out.split('\n').peekable();
                let first = lines.next().unwrap();

                // a one-liner for whatever reason
                // Does not need a new line
                if lines.peek().is_none() {
                    write!(self.buf, "{}: {}", name, first)?;
                } else {
                    writeln!(self.buf, "{}: {}", name, first)?;

                    while let Some(line) = lines.next() {
                        self.indented_tab()?;
                        write!(self.buf, "{}", line)?;
                        if lines.peek().is_none() {
                            write!(self.buf, "")?;
                        } else {
                            writeln!(self.buf)?;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

fn is_short_attrs(attrs: &[ElementAttrNamed]) -> bool {
    let total_attr_len = extract_attr_len(attrs);
    total_attr_len < 80
}

// check if the children are short enough to be on the same line
// We don't have the notion of current line depth - each line tries to be < 80 total
fn is_short_children(children: &[BodyNode]) -> bool {
    if children.is_empty() {
        return true;
    }

    match children {
        [BodyNode::Text(ref text)] => text.value().len() < 80,
        [BodyNode::Element(ref el)] => {
            // && !el.attributes.iter().any(|f| f.attr.is_expr())

            extract_attr_len(&el.attributes) < 80 && is_short_children(&el.children)
        }
        _ => false,
    }
}

fn write_key() {
    // if let Some(key) = key.as_ref().map(|f| f.value()) {
    //     if is_long_attr_list {
    //         self.new_line()?;
    //         self.write_tabs( indent + 1)?;
    //     } else {
    //         write!(self.buf, " ")?;
    //     }
    //     write!(self.buf, "key: \"{key}\"")?;

    //     if !attributes.is_empty() {
    //         write!(self.buf, ",")?;
    //     }
    // }
}
