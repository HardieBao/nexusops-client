import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { useTranslation } from "react-i18next";

interface Status {
  enabled: boolean;
  pending: number;
  uploaded: number;
  last_uploaded_at: string | null;
  last_error: string | null;
}

export function ToolUsagePanel() {
  const { i18n } = useTranslation();
  const zh = i18n.language.startsWith("zh");
  const [status, setStatus] = useState<Status | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  useEffect(() => {
    let active = true;
    const refresh = () =>
      void invoke<Status>("team_tool_usage_status")
        .then((result) => {
          if (active) setStatus(result);
        })
        .catch(() => {
          if (active) setError(true);
        });
    refresh();
    const timer = setInterval(refresh, 15000);
    return () => {
      active = false;
      clearInterval(timer);
    };
  }, []);
  async function action(command: string, args?: { enabled: boolean }) {
    setBusy(true);
    setError(false);
    try {
      setStatus(await invoke<Status>(command, args));
    } catch {
      setError(true);
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="rounded-2xl border bg-card p-5">
      <h3 className="font-semibold">
        {zh ? "工具使用统计 · SkillOps" : "Tool usage · SkillOps"}
      </h3>
      <p className="mt-2 text-sm text-muted-foreground">
        {zh
          ? "启用后，组织管理员可以查看你使用 Codex、Claude Code 的会话数、交互次数、工具调用次数和最后使用时间。只上传工具、事件类型和时间，不上传提示词、代码、对话内容或文件路径。"
          : "When enabled, organization administrators can see your Codex and Claude Code sessions, interactions, tool calls and last activity. Only runtime, event type and time are uploaded. Prompts, code, conversations and file paths stay private."}
      </p>
      <p className="mt-2 text-sm text-muted-foreground">
        {zh
          ? "首次启用会备份并安装工具 Hook。请重启工具，在 /hooks 中确认；受管理策略限制或不支持 Hook 的版本不会产生数据。桌面端运行时每分钟上传，退出组织即停止。"
          : "Enabling backs up settings and installs hooks. Restart your tools and confirm in /hooks. Unsupported versions or managed policies may block collection. Uploads run every minute while the desktop is running; disconnecting stops collection."}
      </p>
      {status && (
        <p className="mt-3 text-sm" aria-live="polite">
          {zh
            ? status.enabled
              ? "采集已启用"
              : "采集已关闭"
            : status.enabled
              ? "Collection enabled"
              : "Collection disabled"}{" "}
          · {zh ? "待上传" : "Pending"} {status.pending} ·{" "}
          {zh ? "已上传事件" : "Uploaded events"} {status.uploaded}
          {status.last_uploaded_at && (
            <>
              {" "}
              · {zh ? "最近上传" : "Last upload"}{" "}
              {new Date(status.last_uploaded_at).toLocaleString()}
            </>
          )}
        </p>
      )}
      {(error || status?.last_error) && (
        <p role="alert" className="mt-2 text-sm text-destructive">
          {zh
            ? "统计操作未完成，请检查组织连接或工具配置后重试。待上传数据会保留；队列满时会停止新增采集。"
            : "Operation incomplete. Check your organization connection or tool settings and retry. Pending events are retained; collection stops when the queue is full."}
        </p>
      )}
      <div className="mt-4 flex flex-wrap gap-2">
        <Button
          disabled={busy || !status}
          variant={status?.enabled ? "outline" : "default"}
          onClick={() =>
            void action("team_configure_tool_usage", {
              enabled: !status?.enabled,
            })
          }
        >
          {zh
            ? status?.enabled
              ? "关闭采集并清空待上传"
              : "启用采集并上传"
            : status?.enabled
              ? "Disable and clear pending"
              : "Enable collection and upload"}
        </Button>
        {status?.enabled && (
          <>
            <Button
              variant="outline"
              disabled={busy}
              onClick={() => void action("team_upload_tool_usage")}
            >
              {zh ? "立即上传" : "Upload now"}
            </Button>
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() =>
                void action("team_configure_tool_usage", { enabled: true })
              }
            >
              {zh
                ? "修复 Hook（切换供应商后）"
                : "Repair hooks after provider changes"}
            </Button>
          </>
        )}
      </div>
    </section>
  );
}
