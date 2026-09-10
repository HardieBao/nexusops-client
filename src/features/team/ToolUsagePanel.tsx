import { useEffect, useRef, useState } from "react";
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

function errorCode(error: unknown): string {
  if (
    error &&
    typeof error === "object" &&
    "code" in error &&
    typeof error.code === "string" &&
    [
      "upgrade_required",
      "authentication_required",
      "access_denied",
      "rate_limited",
    ].includes(error.code)
  )
    return error.code;
  return "request";
}

export function ToolUsagePanel({
  disabled = false,
  onBusyChange,
}: { disabled?: boolean; onBusyChange?: (busy: boolean) => void } = {}) {
  const { i18n } = useTranslation();
  const zh = i18n.language.startsWith("zh");
  const [status, setStatus] = useState<Status | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const revision = useRef(0);
  const alive = useRef(true);
  const working = useRef(false);
  const disabledRef = useRef(disabled);
  disabledRef.current = disabled;
  useEffect(() => () => onBusyChange?.(false), [onBusyChange]);
  useEffect(() => {
    let active = true;
    alive.current = true;
    const refresh = (initial = false) => {
      if (
        !initial &&
        (working.current ||
          disabledRef.current ||
          document.visibilityState === "hidden")
      )
        return;
      const current = ++revision.current;
      void invoke<Status>("team_tool_usage_status")
        .then((result) => {
          if (active && current === revision.current) setStatus(result);
        })
        .catch((failure) => {
          if (active && current === revision.current)
            setError(errorCode(failure));
        });
    };
    refresh(true);
    const timer = setInterval(() => refresh(), 15000);
    return () => {
      active = false;
      alive.current = false;
      revision.current++;
      clearInterval(timer);
    };
  }, []);
  async function action(command: string, args?: { enabled: boolean }) {
    if (working.current || disabled) return;
    working.current = true;
    const current = ++revision.current;
    setBusy(true);
    onBusyChange?.(true);
    setError(null);
    try {
      const result = await invoke<Status>(command, args);
      if (alive.current && current === revision.current) setStatus(result);
    } catch (failure) {
      if (alive.current && current === revision.current)
        setError(errorCode(failure));
    } finally {
      working.current = false;
      if (alive.current) {
        setBusy(false);
        onBusyChange?.(false);
      }
    }
  }
  const failure = errorCode({ code: error || status?.last_error });
  const errorMessages: Record<string, string> = {
    upgrade_required: zh
      ? "请升级组织网关后再启用或修复采集；本机历史会保留。"
      : "Upgrade the gateway before enabling or repairing collection. Local history is retained.",
    authentication_required: zh
      ? "成员凭据已失效，请使用有效 Key 重新连接组织。"
      : "Your member credential is no longer valid. Reconnect with a valid key.",
    access_denied: zh
      ? "组织已拒绝访问，请联系管理员核对成员权限。"
      : "The organization denied access. Ask an administrator to check your permissions.",
    rate_limited: zh
      ? "网关暂时限流，请稍后重试；待上传事件会保留。"
      : "The gateway is rate limiting requests. Retry later; pending events are retained.",
  };
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
          {(failure && errorMessages[failure]) ||
            (zh
              ? "统计操作未完成，请检查组织连接或工具配置后重试。待上传数据会保留；队列满时会停止新增采集。"
              : "Operation incomplete. Check your organization connection or tool settings and retry. Pending events are retained; collection stops when the queue is full.")}
        </p>
      )}
      <div className="mt-4 flex flex-wrap gap-2">
        <Button
          disabled={busy || disabled || !status}
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
              disabled={busy || disabled}
              onClick={() => void action("team_upload_tool_usage")}
            >
              {zh ? "立即上传" : "Upload now"}
            </Button>
            <Button
              variant="ghost"
              disabled={busy || disabled}
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
