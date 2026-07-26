import { useEffect, useRef } from "react";

interface ContextMenuProps {
    x: number;
    y: number;
    onClose: () => void;
    onExportMemory: () => void;
    onAwayMode: () => void;
    onQuit: () => void;
    onDevTools: () => void;
}

export function ContextMenu({ x, y, onClose, onExportMemory, onAwayMode, onQuit, onDevTools }: ContextMenuProps) {
    const ref = useRef<HTMLDivElement>(null);

    useEffect(() => {
        const handler = (e: MouseEvent) => {
            if (ref.current && !ref.current.contains(e.target as Node)) {
                onClose();
            }
        };
        const escHandler = (e: KeyboardEvent) => {
            if (e.key === "Escape") onClose();
        };
        document.addEventListener("mousedown", handler);
        document.addEventListener("keydown", escHandler);
        return () => {
            document.removeEventListener("mousedown", handler);
            document.removeEventListener("keydown", escHandler);
        };
    }, [onClose]);

    const clampedX = Math.min(x, window.innerWidth - 180);
    const clampedY = Math.min(y, window.innerHeight - 160);

    return (
        <div
            ref={ref}
            className="context-menu"
            style={{ left: clampedX, top: clampedY }}
        >
            <button className="context-menu-item" onClick={() => { onExportMemory(); onClose(); }}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3" />
                </svg>
                <span>导出记忆</span>
            </button>
            <button className="context-menu-item" onClick={() => { onAwayMode(); onClose(); }}>
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <circle cx="12" cy="12" r="10" />
                    <path d="M12 6v6l4 2" />
                </svg>
                <span>暂时离开</span>
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
