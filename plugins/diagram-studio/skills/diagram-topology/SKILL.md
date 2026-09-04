---
name: diagram-topology
description: Design one readable Diagram Studio deployment or network topology view with real runtime boundaries, explicit traffic direction, evidence, complexity limits, and PlantUML deployment guidance.
metadata:
  chatos.role: leaf
---

# Topology diagram generation

A topology diagram explains where software runs and how traffic crosses deployment or network boundaries. It is not a logical service catalog and it must not invent replicas, zones, protocols, or infrastructure that the evidence does not support.

## Scope

Use the `deployment` mode for one environment or one traffic question, such as production request routing, an on-premise-to-cloud connection, cluster ingress, service-to-database reachability, or disaster-recovery traffic.

- Keep no more than 15 primary runtime elements and 24 relationships.
- Use at least two real structure groups when the source describes separate environments, networks, clusters, namespaces, zones, hosts, or trust boundaries.
- Split production and disaster recovery, internal control traffic and public request traffic, or several unrelated regions when combining them makes the path ambiguous.
- Use an architecture diagram for responsibilities and bounded capabilities. Do not redraw the logical architecture as deployment boxes with no runtime evidence.

## Evidence before drawing

Inspect deployment manifests, Compose files, Helm/Kubernetes resources, ingress or gateway configuration, ports, service discovery, database endpoints, cloud resources, and network policy. Map every code- or configuration-derived alias to `sourceReferences`. Mark uncertain infrastructure as an assumption instead of presenting it as fact.

## Boundaries and elements

- `cloud`: an external network, managed platform, or third-party boundary.
- `node`: a host, VM, cluster node, runtime group, or other real execution boundary.
- `package`, `frame`, or deployment container: an environment, network, cluster, namespace, zone, or trust boundary.
- `component` or `artifact`: a deployed workload only when its runtime placement matters.
- `database`, `storage`, `queue`: a deployed or managed dependency reached by the shown traffic.

Do not add one box per pod, replica, port, or configuration key unless the selected question specifically depends on that distinction. Redundancy must reflect actual replicas, zones, or failover paths.

## Relationships and layout

- Show traffic direction and label important links with protocol, port, or purpose, such as `HTTPS 443`, `gRPC`, `SQL`, `Publish`, or `VPN`.
- Distinguish public ingress, internal service traffic, asynchronous delivery, administration, and replication when they matter.
- Prefer left-to-right flow from caller/network edge to runtime workloads and managed dependencies.
- Keep deployment groups visually separate. Avoid lines through group titles and avoid hiding a primary path behind monitoring or backup links.
- Use dashed lines for optional, asynchronous, management, or replication relationships when that meaning is accurate.

## PlantUML

Use PlantUML deployment constructs such as `cloud`, `node`, `database`, `storage`, `queue`, `artifact`, `package`, and `frame`. Give every structural declaration a unique ASCII alias and reference aliases in relationships. Keep visible labels concise and put configuration paths in `nodeEvidence`.

Read [positive and negative topology examples](references/examples.md) before submitting the generation plan.

## Checklist

- `single_environment_or_traffic_question`: the diagram answers one deployment or traffic question.
- `deployment_boundaries_are_real`: groups correspond to evidenced runtime, network, or trust boundaries.
- `traffic_direction_is_visible`: the primary traffic path and protocol meaning are readable.
- `redundancy_is_not_fake_detail`: replicas, zones, and failover paths are shown only when verified and relevant.
- `logical_architecture_is_separated`: logical capability detail is kept in architecture diagrams.
- `configuration_evidence_is_mapped`: configuration-derived elements have source references.

Do not put every environment, region, cluster, namespace, workload, database, monitoring system, and backup path into one topology. Build a small topology set when several operational questions must be answered.
