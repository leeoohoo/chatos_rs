namespace ChatOS.Core.Domain;

public sealed record LocalProjectTypeDefinition(
    string Key,
    string Label,
    string LabelEnglish,
    string Description,
    string DescriptionEnglish,
    string CategoryLabel,
    string CategoryLabelEnglish);

/// <summary>
/// Product-owned project types mirrored from the macOS ChatOS Skill catalog.
/// Stable keys are persisted with local projects; labels are presentation metadata only.
/// </summary>
public static class LocalProjectTypeCatalog
{
    public const string LegacyProjectTypeKey = "software_development";

    public static IReadOnlyList<LocalProjectTypeDefinition> All { get; } =
    [
        Type("software_development", "通用软件开发", "General Software Development", "无法进一步归入明确形态的软件产品、服务或工程项目。", "Software products or engineering work that cannot yet be classified into a more specific delivery shape.", "软件与数字产品", "Software & Digital Products"),
        Type("web_application", "Web 应用与网站", "Web Applications & Websites", "面向浏览器的业务应用、门户、管理后台、官网和全栈 Web 产品。", "Browser-based business applications, portals, admin consoles, public sites, and full-stack web products.", "软件与数字产品", "Software & Digital Products"),
        Type("mobile_application", "移动端应用", "Mobile Applications", "iOS、Android、Flutter、React Native、PDA 和移动设备应用。", "iOS, Android, Flutter, React Native, PDA, and other device-oriented applications.", "软件与数字产品", "Software & Digital Products"),
        Type("desktop_application", "桌面应用", "Desktop Applications", "Windows、macOS、Linux 桌面软件、客户端和跨平台桌面产品。", "Windows, macOS, and Linux desktop clients and cross-platform desktop products.", "软件与数字产品", "Software & Digital Products"),
        Type("backend_service", "后端服务与 API", "Backend Services & APIs", "微服务、单体后端、开放 API、网关和集成服务。", "Monoliths, microservices, public APIs, gateways, and integration services.", "软件与数字产品", "Software & Digital Products"),
        Type("library_sdk", "库、SDK 与开发工具", "Libraries, SDKs & Developer Tools", "公共库、SDK、CLI、编译工具、插件和开发者基础设施。", "Reusable libraries, SDKs, CLIs, compilers, plugins, and developer infrastructure.", "软件与数字产品", "Software & Digital Products"),
        Type("game_development", "游戏开发", "Game Development", "2D/3D 游戏、互动体验、关卡、游戏服务和内容工具链。", "2D/3D games, interactive experiences, levels, game services, and content pipelines.", "软件与数字产品", "Software & Digital Products"),
        Type("iot_embedded_system", "IoT 与嵌入式系统", "IoT & Embedded Systems", "设备固件、边缘计算、工业协议、硬件控制和设备云。", "Firmware, edge computing, industrial protocols, hardware control, and device-cloud systems.", "软件与数字产品", "Software & Digital Products"),
        Type("enterprise_erp", "ERP 企业资源计划", "ERP Enterprise Resource Planning", "覆盖财务、采购、销售、库存、制造、人力或项目核算的企业系统。", "Enterprise systems spanning finance, procurement, sales, inventory, manufacturing, HR, or project accounting.", "企业业务系统", "Enterprise Business Systems"),
        Type("warehouse_management_system", "WMS 仓储管理系统", "WMS Warehouse Management System", "覆盖入库、上架、库存、补货、波次、拣选、复核、出库和盘点的仓储系统。", "Warehouse systems covering receiving, putaway, inventory, replenishment, waves, picking, checking, shipping, and counting.", "企业业务系统", "Enterprise Business Systems"),
        Type("customer_relationship_management", "CRM 客户关系管理", "CRM Customer Relationship Management", "覆盖线索、客户、联系人、商机、报价、活动和客户成功的业务系统。", "Systems for leads, accounts, contacts, opportunities, quotes, activities, and customer success.", "企业业务系统", "Enterprise Business Systems"),
        Type("manufacturing_execution_system", "MES 制造执行系统", "MES Manufacturing Execution System", "覆盖工单、BOM、工艺、在制品、质量、追溯、设备和生产绩效的制造系统。", "Manufacturing systems for work orders, BOM, routing, WIP, quality, traceability, equipment, and production performance.", "企业业务系统", "Enterprise Business Systems"),
        Type("ecommerce_platform", "电商与交易平台", "E-commerce & Transaction Platforms", "商城、交易中台、订单、支付、促销、履约、售后和多渠道零售系统。", "Storefronts and commerce platforms for catalog, cart, checkout, payment, promotion, fulfillment, and after-sales service.", "企业业务系统", "Enterprise Business Systems"),
        Type("data_analysis", "数据分析", "Data Analysis", "数据清洗、探索分析、指标体系、报表、可视化和决策结论。", "Data cleaning, exploratory analysis, metrics, reports, visualization, and decision conclusions.", "数据、AI 与研究", "Data, AI & Research"),
        Type("data_engineering_platform", "数据工程与平台", "Data Engineering & Platforms", "数据仓库、湖仓、ETL/ELT、流处理、调度、数据质量和数据服务。", "Warehouses, lakehouses, ETL/ELT, streaming, orchestration, data quality, and data services.", "数据、AI 与研究", "Data, AI & Research"),
        Type("machine_learning_system", "机器学习与模型系统", "Machine Learning & Model Systems", "训练、评估、推理、推荐、搜索、预测和 MLOps 系统。", "Training, evaluation, inference, recommendation, search, forecasting, and MLOps systems.", "数据、AI 与研究", "Data, AI & Research"),
        Type("research", "研究调研", "Research & Investigation", "资料调研、竞品分析、学术研究、行业研究和可行性论证。", "Literature, competitive, academic, industry, and feasibility research.", "数据、AI 与研究", "Data, AI & Research"),
        Type("product_design", "产品与体验设计", "Product & Experience Design", "产品方案、用户旅程、信息架构、交互、视觉和服务设计。", "Product concepts, journeys, information architecture, interaction, visual, and service design.", "设计、内容与知识", "Design, Content & Knowledge"),
        Type("design_system_brand", "设计系统与品牌", "Design Systems & Brand", "设计 Token、组件库、品牌识别、视觉规范和多端一致性建设。", "Design tokens, component libraries, brand identity, visual standards, and cross-platform consistency.", "设计、内容与知识", "Design, Content & Knowledge"),
        Type("novel_writing", "小说创作", "Novel Writing", "长篇、短篇、连载小说和其他叙事作品。", "Long-form, short-form, serialized, and other narrative fiction.", "设计、内容与知识", "Design, Content & Knowledge"),
        Type("general_writing", "专业写作", "Professional Writing", "文章、报告、白皮书、演讲稿、脚本和非虚构内容。", "Articles, reports, white papers, speeches, scripts, and nonfiction content.", "设计、内容与知识", "Design, Content & Knowledge"),
        Type("documentation", "文档与知识库", "Documentation & Knowledge Bases", "产品文档、API 文档、操作手册、教程、规范和知识库。", "Product docs, API references, operating manuals, tutorials, standards, and knowledge bases.", "设计、内容与知识", "Design, Content & Knowledge"),
        Type("marketing_content", "市场、品牌与增长内容", "Marketing, Brand & Growth Content", "营销活动、品牌传播、社媒、增长实验和销售内容。", "Campaigns, brand communication, social content, growth experiments, and sales enablement.", "设计、内容与知识", "Design, Content & Knowledge"),
        Type("automation", "自动化、Agent 与集成", "Automation, Agents & Integrations", "工作流、脚本、Agent、MCP、机器人和重复任务自动化。", "Workflows, scripts, agents, MCP services, robots, and repeated-task automation.", "自动化、运营与交付", "Automation, Operations & Delivery"),
        Type("operations", "运营与持续服务", "Operations & Continuous Services", "产品运营、平台运营、内容运营、客户运营和持续服务。", "Product, platform, content, customer, and other ongoing operational services.", "自动化、运营与交付", "Automation, Operations & Delivery"),
        Type("implementation_migration", "实施、迁移与上线", "Implementation, Migration & Go-live", "企业软件实施、数据迁移、系统切换、培训、UAT 和上线交接。", "Enterprise implementation, data migration, system cutover, training, UAT, and operational handover.", "自动化、运营与交付", "Automation, Operations & Delivery"),
        Type("general", "通用项目", "General Project", "尚不属于其他明确类别的协作项目。", "A collaborative project that does not yet fit a more specific category.", "通用项目", "General Projects"),
    ];

    public static LocalProjectTypeDefinition? Find(string key) =>
        All.FirstOrDefault(value => string.Equals(value.Key, key, StringComparison.Ordinal));

    private static LocalProjectTypeDefinition Type(
        string key,
        string label,
        string labelEnglish,
        string description,
        string descriptionEnglish,
        string categoryLabel,
        string categoryLabelEnglish) =>
        new(key, label, labelEnglish, description, descriptionEnglish, categoryLabel, categoryLabelEnglish);
}
