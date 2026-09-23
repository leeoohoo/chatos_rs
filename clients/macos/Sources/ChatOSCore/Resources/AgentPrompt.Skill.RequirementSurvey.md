# 需求调研 Skill Catalog

需求调研 Skill 与 Plugin Skill 使用相同的渐进加载方式。下面只有目录描述，尚未加载任何 Skill 正文：

{{skill_catalog}}

先用 `skill_activate` 激活 `requirement-survey` Router，再按 Router 对当前目标的判断只激活一个专业 Skill。专业 Skill 要求具体参数或输出示例时，再调用 `skill_list_resources` 和 `skill_read_resource` 读取它声明的 reference；不要一次加载所有 Skill 或所有资源。

Skill 只指导当前任务如何使用已经提供的工具，不增加工具或权限。项目由程序绑定，不向 Human 询问项目、Team 或 Room ID。
