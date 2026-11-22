use fast5ever::{DOCUMENT, NodeData, parse_fragment};
use html5ever::ns;

#[test]
fn element_queries_are_shallow_and_namespace_aware() {
    let dom = parse_fragment("text<!--c--><a class='one two'></a><svg><a/></svg><template><b/></template>", "body");
    let els = dom.element_children(DOCUMENT);
    assert_eq!(els.len(), 3);
    let (a, svg, template) = (els[0], els[1], els[2]);
    let foreign = dom.element_children(svg)[0];
    assert_eq!(dom.tag(a), dom.tag(foreign));
    assert!(dom.is_tag(a, "a", None));
    assert!(!dom.is_tag(foreign, "a", None));
    assert!(dom.is_tag(foreign, "a", Some("http://www.w3.org/2000/svg")));
    assert!(dom.has_class(a, "two"));
    assert!(!dom.has_class(a, "tw"));
    assert_eq!(dom.tag(DOCUMENT), None);
    assert!(!dom.is_tag(DOCUMENT, "a", None));
    assert!(!dom.has_class(DOCUMENT, "one"));
    assert!(dom.element_children(template).is_empty());
    let NodeData::Element { template_contents: Some(content), .. } = dom.get(template).data else { panic!("expected template") };
    assert!(dom.is_tag(dom.element_children(content)[0], "b", None));
}

#[test]
fn qualified_attr_mutation_preserves_namespace() {
    let mut dom = parse_fragment("<svg><a xlink:href='foreign'></a></svg>", "body");
    let svg = dom.children(DOCUMENT)[0];
    let a = dom.children(svg)[0];
    let NodeData::Element { attrs, .. } = &dom.get(a).data else { panic!("expected element") };
    let name = attrs[0].0.clone();
    assert_eq!(name.ns, ns!(xlink));
    assert_eq!(dom.attr(a, "href"), None);
    assert_eq!(dom.remove_attr(a, "href"), Ok(None));
    dom.set_attr(a, "href", "local").unwrap();
    dom.set_attr(a, "xlink:href", "changed").unwrap();
    let NodeData::Element { attrs, .. } = &dom.get(a).data else { panic!("expected element") };
    assert_eq!(attrs[0].0, name);
    assert_eq!(attrs[1].0.ns, ns!());
    assert_eq!(dom.attr(a, "xlink:href"), Some("changed"));
    assert_eq!(dom.remove_attr(a, "xlink:href"), Ok(Some("changed".into())));
    assert_eq!(dom.attr(a, "href"), Some("local"));
}
