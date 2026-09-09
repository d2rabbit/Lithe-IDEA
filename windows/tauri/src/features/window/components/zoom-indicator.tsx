import { AnimatePresence, motion } from "motion/react";
import { useZoomStore } from "@/features/window/stores/zoom.store";
import { useTranslation } from "@/i18n/locale-provider";
import { quickTransition } from "@/utils/motion";

export function ZoomIndicator() {
  const { t } = useTranslation();
  const showZoomIndicator = useZoomStore.use.showZoomIndicator();
  const zoomIndicatorType = useZoomStore.use.zoomIndicatorType();
  const editorZoomLevel = useZoomStore.use.editorZoomLevel();
  const terminalZoomLevel = useZoomStore.use.terminalZoomLevel();

  const zoomLevel = zoomIndicatorType === "editor" ? editorZoomLevel : terminalZoomLevel;
  const label = zoomIndicatorType === "editor" ? t("editor.title") : t("workbench.terminal");

  return (
    <AnimatePresence>
      {showZoomIndicator && zoomIndicatorType ? (
        <motion.div
          key="zoom-indicator"
          initial={{ opacity: 0, y: -6 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -6 }}
          transition={quickTransition}
          className="fixed top-4 right-4 z-50 rounded bg-black/80 px-2 py-1 text-white ui-text-sm backdrop-blur-sm"
        >
          {label}: {Math.round(zoomLevel * 100)}%
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
