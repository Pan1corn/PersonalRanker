import { useEffect, useRef, useState } from "react";
import { loadMediaPreview, type MediaPreview } from "./media-api";

interface Props {
  projectPath: string;
  itemId: string;
  fieldName: string;
  value: unknown;
  allowNetworkImages: boolean;
  compact?: boolean;
}

export function MediaThumbnail({
  projectPath,
  itemId,
  fieldName,
  value,
  allowNetworkImages,
  compact = false,
}: Props) {
  const [preview, setPreview] = useState<MediaPreview | null>(null);
  const [failed, setFailed] = useState(false);
  const timeoutRef = useRef<number | null>(null);
  const candidate = typeof value === "string" && looksLikeImage(value);

  useEffect(() => {
    if (!candidate) return;
    let active = true;
    setPreview(null);
    setFailed(false);
    void loadMediaPreview({
      projectPath,
      itemId,
      fieldName,
      allowNetwork: allowNetworkImages,
    })
      .then((result) => active && setPreview(result))
      .catch(() => active && setFailed(true));
    return () => {
      active = false;
      if (timeoutRef.current !== null) window.clearTimeout(timeoutRef.current);
    };
  }, [allowNetworkImages, candidate, fieldName, itemId, projectPath]);

  useEffect(() => {
    if (preview?.status !== "remote_ready") return;
    timeoutRef.current = window.setTimeout(() => setFailed(true), 8_000);
    return () => {
      if (timeoutRef.current !== null) window.clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    };
  }, [preview]);

  if (!candidate) return null;
  if (failed || preview?.status === "missing") {
    return <MediaPlaceholder compact={compact}>图片不可用</MediaPlaceholder>;
  }
  if (preview?.status === "too_large") {
    return <MediaPlaceholder compact={compact}>图片超过 20 MB</MediaPlaceholder>;
  }
  if (preview?.status === "remote_blocked") {
    return <MediaPlaceholder compact={compact}>网络图片未启用</MediaPlaceholder>;
  }
  if (!preview) return <MediaPlaceholder compact={compact}>正在读取图片…</MediaPlaceholder>;
  const source = preview.status === "local_ready" ? preview.dataUrl : preview.url;
  return (
    <img
      className={`media-thumbnail${compact ? " compact" : ""}`}
      src={source}
      alt={`${fieldName}缩略图`}
      loading="lazy"
      referrerPolicy="no-referrer"
      onLoad={() => {
        if (timeoutRef.current !== null) window.clearTimeout(timeoutRef.current);
        timeoutRef.current = null;
      }}
      onError={() => setFailed(true)}
    />
  );
}

function MediaPlaceholder({ compact, children }: { compact: boolean; children: string }) {
  return (
    <span className={`media-placeholder${compact ? " compact" : ""}`}>
      <span>▧</span>
      {children}
    </span>
  );
}

function looksLikeImage(value: string): boolean {
  const trimmed = value.trim();
  const target = trimmed.startsWith("![")
    ? (trimmed.match(/^!\[[^\]]*\]\((.+?)\)/)?.[1] ?? "")
    : trimmed;
  return /\.(png|jpe?g|gif|webp|bmp|svg)(?:[?#].*)?$/i.test(target);
}
