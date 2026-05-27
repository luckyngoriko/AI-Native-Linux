"use client";

import React, { useEffect, useRef } from "react";

/**
 * ShadowRootHost attaches a closed shadow root to its host element
 * and renders chrome-zone components into it.
 *
 * INV I2: closed shadow root isolates chrome UI from page content.
 * INV I7: chrome integrity barrier — page scripts cannot inspect chrome DOM.
 */
const ShadowRootHost: React.FC<{ children?: React.ReactNode }> = ({
  children,
}) => {
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host || host.shadowRoot) return;

    const root = host.attachShadow({ mode: "closed" });
    // T-149 wires the full chrome rendering into the closed shadow root.
    // For now, the host is present in the layout at z-index 9999.
    void root;
  }, []);

  return (
    <div
      ref={hostRef}
      id="aios-chrome-shadow-root-host"
      style={{ position: "fixed", top: 0, left: 0, zIndex: 9999 }}
    >
      {children}
    </div>
  );
};

export default ShadowRootHost;
