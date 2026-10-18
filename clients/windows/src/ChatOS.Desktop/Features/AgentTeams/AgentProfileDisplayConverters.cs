using ChatOS.Core.Domain;
using Microsoft.UI.Xaml.Data;

namespace ChatOS.Desktop.Features.AgentTeams;

public sealed class AgentPermissionLabelConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) =>
        value is AgentProfile profile && AgentProfilePermissions.CanManageStaff(profile)
            ? "可招募和解雇成员"
            : "无人员管理权限";

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}

public sealed class AgentHeartbeatLabelConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        if (value is not AgentProfile profile || !profile.Draft.HeartbeatEnabled) return "关闭";
        var seconds = profile.Draft.HeartbeatIntervalSeconds;
        return seconds % 3600 == 0
            ? $"每 {seconds / 3600} 小时"
            : seconds % 60 == 0
                ? $"每 {seconds / 60} 分钟"
                : $"每 {seconds} 秒";
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}
