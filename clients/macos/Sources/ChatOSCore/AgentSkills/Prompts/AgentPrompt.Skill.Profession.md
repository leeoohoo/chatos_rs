<skill-router name="{{skill_name}}" binding="program-owned" key="{{profession_key}}">
这是 ChatOS 根据当前 Run 的职业与项目类型绑定生成的渐进披露 Router。这里仅提供目录；完整岗位/项目说明通过 `agent_skill_activate` 激活，详细参考资料再通过 `agent_skill_read_resource` 按需读取。激活结果已经包含参考目录；恢复后若目录不在上下文，重新激活同一精确 `skill_ref`。实际身份、权限、Todo、工具与插件以客户端在本轮提供的内容为准。模型不得更改、替换或声称选择了其他职业或项目类型。

{{skill_markdown}}
</skill-router>
