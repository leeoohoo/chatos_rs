# Mind-map generation examples

## Positive: one product question

Question: What capabilities make up Diagram Studio?

```text
Diagram Studio
├─ Manual editing
│  ├─ Component library
│  ├─ Multi-selection
│  └─ Layers
├─ AI generation
│  ├─ Dedicated guides
│  ├─ Generation permits
│  └─ Quality validation
├─ Interoperability
│  ├─ PlantUML import
│  └─ PlantUML export
└─ Delivery
   ├─ Plugin package
   └─ Local UI runtime
```

Why it works: the center is specific, primary branches partition the capability question, labels are short, and no branch pretends to show time or dependencies.

## Positive: split a large business domain

Do not create one “商城全部业务” map. Create a small set instead:

```text
商城业务地图
  customer journeys
  merchant operations
  platform governance

订单履约知识图
  order states
  inventory concepts
  delivery concepts
  exception concepts

售后决策问题图
  refund reasons
  evidence
  policy questions
  manual review
```

Each map has one subject and can be read without shrinking text.

## Negative: the entire repository under one root

```text
ChatOS
  every page
  every API
  every database table
  every Rust module
  every plugin
  every deployment node
  every business process
  every open issue
```

Why it fails: the first-level branches mix product, code, deployment, process, and planning viewpoints. Replace it with several diagrams, including architecture and flow diagrams where those semantics fit better.

## Negative: paragraph topics

```text
Authentication
  The client sends the access token to the gateway and if it has expired the gateway will...
```

Why it fails: the child is prose and describes sequence. Use short knowledge topics such as `access token`, `refresh token`, `expiry`, and `revocation`, or use a sequence diagram for request timing.

## Negative: graph disguised as a mind map

```text
root -> service A
service A -> database
database -> service B
service B -> service A
```

Why it fails: cycles and cross-dependencies are architecture semantics. Every mind-map topic must have one parent, branches have no arrowheads, and the graph must contain exactly `topics - 1` edges.

## Negative: duplicate categories

```text
Release readiness
  quality
    tests
  engineering
    tests
  delivery
    tests
```

Why it fails: the same concept is repeated below several parents without clarifying distinct meanings. Make branch boundaries mutually exclusive, or create a separate test-readiness map.
