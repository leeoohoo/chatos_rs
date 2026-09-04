# Swimlane examples

## Positive: purchase-receipt collaboration

Lanes: 采购员、采购服务、仓库人员、库存服务.

Flow: procurement submits a receipt notice; the procurement service validates it; warehouse staff confirms batch and location; inventory records stock; procurement closes the receipt task.

Why it works: every lane owns a distinct responsibility and the diagram has one completion outcome.

## Negative: technical layers used as lanes

Bad lanes: Controller、Service、Repository、Database, containing an application's method calls. This is architecture or sequence detail, not process ownership. Use an architecture detail or sequence diagram instead.

## Negative: whole-company process

Bad: one diagram includes sales intake, hiring, payroll, procurement, production, delivery, support, and financial closing. Repair by creating one swimlane per business outcome and an architecture overview for system relationships.
