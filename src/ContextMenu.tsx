import { useEffect, useRef, useState } from "react";

interface ContextMenuProps {
    x: number;
    y: number;
    onClose: () => void;
    onExportJson: () => void;
    onExportMarkdown: () => void;
    onExportBoth: () => void;
    onAwayMode: () => void;
    soundMuted: boolean;
    onToggleSound: () => void;
    onOpenSettings: () => void;
    onQuit: () => void;
    onDevTools: () => void;
}

export function ContextMenu({ x, y, onClose, onExportJson, onExportMarkdown, onExportBoth, onAwayMode, soundMuted, onToggleSound, onOpenSettings, onQuit, onDevTools }: ContextMenuProps) {
    const ref = useRef<HTMLDivElement>(null);
    const [exportOpen, setExportOpen] = useState(false);

    useEffect(() => {
        const handler = (e: MouseEvent) => {
            if (ref.current && !ref.current.contains(e.target as Node)) {
                onClose();
            }
        };
        const escHandler = (e: KeyboardEvent) => {
            if (e.key === "Escape") {
                if (exportOpen) {
                    setExportOpen(false);
                } else {
                    onClose();
                }
            }
        };
        document.addEventListener("mousedown", handler);
        document.addEventListener("keydown", escHandler);
        return () => {
            document.removeEventListener("mousedown", handler);
            document.removeEventListener("keydown", escHandler);
        };
    }, [onClose, exportOpen]);

    const clampedX = Math.min(x, window.innerWidth - 180);
    const clampedY = Math.min(y, window.innerHeight - 160);

    const pickExport = (fn: () => void) => {
        setExportOpen(false);
        fn();
        onClose();
    };

    return (
        <div
            ref={ref}
            className="context-menu"
            style={{ left: clampedX, top: clampedY }}
        >
            <button className="context-menu-item" onClick={() => setExportOpen(v => !v)}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3" />
                </svg>
                <span>导出记忆</span>
                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" style={{ marginLeft: "auto", opacity: 0.5 }}>
                    <path d="M9 18l6-6-6-6" />
                </svg>
            </button>
            {exportOpen && (
                <div className="context-submenu">
                    <button className="context-submenu-item" onClick={() => pickExport(onExportJson)}>
                        <div className="context-submenu-main">JSON 备份</div>
                        <div className="context-submenu-desc">完整数据，可恢复</div>
                    </button>
                    <button className="context-submenu-item" onClick={() => pickExport(onExportMarkdown)}>
                        <div className="context-submenu-main">Markdown</div>
                        <div className="context-submenu-desc">方便阅读，核心记忆</div>
                    </button>
                    <button className="context-submenu-item" onClick={() => pickExport(onExportBoth)}>
                        <div className="context-submenu-main">两个都要</div>
                        <div className="context-submenu-desc">同时保存两份</div>
                    </button>
                </div>
            )}
            <button className="context-menu-item" onClick={() => { onAwayMode(); onClose(); }}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <circle cx="12" cy="12" r="10" />
                    <path d="M12 6v6l4 2" />
                </svg>
                <span>暂时离开</span>
            </button>
            <button className="context-menu-item" onClick={() => { onToggleSound(); onClose(); }}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M11 5L6 9H2v6h4l5 4V5z" />
                    {soundMuted ? (
                        <line x1="22" y1="9" x2="16" y2="15" />
                    ) : (
                        <path d="M19.07 4.93a10 10 0 010 14.14M15.54 8.46a5 5 0 010 7.07" />
                    )}
                </svg>
                <span>{soundMuted ? "开启声音" : "静音"}</span>
            </button>
            <button className="context-menu-item" onClick={() => { onOpenSettings(); onClose(); }}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <circle cx="12" cy="12" r="3" />
                    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
                </svg>
                <span>模型与设置</span>
            </button>
            <div className="context-menu-divider" />
            <button className="context-menu-item" onClick={() => { onDevTools(); onClose(); }}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M16 18l6-6-6-6M8 6l-6 6 6 6" />
                </svg>
                <span>开发者工具</span>
            </button>
            <button className="context-menu-item context-menu-danger" onClick={() => { onQuit(); onClose(); }}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M18 6L6 18M6 6l12 12" />
                </svg>
                <span>关闭</span>
            </button>
        </div>
    );
}
