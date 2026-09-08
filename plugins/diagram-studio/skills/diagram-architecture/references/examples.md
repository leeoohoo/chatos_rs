# Architecture examples

## Positive: bounded system overview

```plantuml
@startuml
left to right direction
actor "业务用户" as business_user
package "客户端" as client_layer { component "Web 管理端" as web_app }
package "接入层" as entry_layer { component "API Gateway" as api_gateway }
package "业务能力" as domain_layer {
  component "订单域" as order_domain
  component "库存域" as inventory_domain
  component "生产域" as production_domain
}
package "数据与基础设施" as data_layer {
  database "业务数据" as business_data
  queue "可靠任务" as task_queue
}
business_user --> web_app : Uses
web_app --> api_gateway : HTTPS
api_gateway --> order_domain : Routes
api_gateway --> inventory_domain : Routes
api_gateway --> production_domain : Routes
order_domain --> inventory_domain : Reserve stock
order_domain --> business_data : Persists
production_domain ..> task_queue : Publish
@enduml
```

Why it works: one system-level viewpoint, real boundaries, a visible request path, aggregated data responsibilities, and no implementation classes.

## Negative: ChatOS everything-at-once overview

Bad: one canvas contains Client, ChatOS Backend, model provider, MCP Management, Plugin Management, Task Runner, execution environment, project memory, MongoDB, RabbitMQ, plus separate create/query, callback, persistence, synchronization, publish, and consume relationships.

Why it fails: it mixes a system-context view with Agent runtime and background-task process details. Reciprocal ChatOS ↔ Task Runner edges create a layout cycle, while both services connecting directly to every data dependency creates parallel edge stars.

Repair as a diagram set:

1. `ChatOS system context`: User → Client → ChatOS Core, with one relationship each to Model Provider, Tool Runtime, Background Tasks, and Data & Messaging.
2. `Agent and MCP capability`: ChatOS Core, MCP Management, Plugin Management, and local/sandbox execution.
3. `Background task execution`: ChatOS Core, Task Runner, execution environment, message broker, and result delivery.

Do not preserve every relationship by merely spreading the nodes farther apart. Remove or aggregate relationships before layout.

## Negative: every implementation detail on one canvas

Bad: a flat graph containing every User/Order/Inventory Controller, Service, Repository, table, endpoint, queue, and deployment node.

Why it fails: it has no single architectural question, mixes levels, and turns shared gateway/database nodes into unreadable edge stars.

Repair: create one overview containing User, Order, and Inventory as capabilities. Create separate detail diagrams only for domains whose internal design matters.

## Negative: repeated gateway and database stars

Bad: connect the gateway to every Controller and every Service/Repository to the same database symbol.

Repair: connect the gateway once to each bounded capability and each capability once to its owned data responsibility. Put endpoint and repository detail in separate diagrams.
