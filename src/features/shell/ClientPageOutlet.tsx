import type { ReactNode } from "react";
import { AnimatePresence, motion } from "framer-motion";
import type { ClientView } from "./navigation";

export function ClientPageOutlet({
  view,
  children,
}: {
  view: ClientView;
  children: ReactNode;
}) {
  return (
    <AnimatePresence mode="wait">
      <motion.div
        key={view}
        className="flex-1 min-h-0"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        transition={{ duration: 0.2 }}
      >
        {children}
      </motion.div>
    </AnimatePresence>
  );
}
