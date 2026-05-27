import React from "react";

export interface EvidenceLinkProps {
  id: string;
  children?: React.ReactNode;
}

/**
 * chrome-zone: closed shadow root rendering happens at the
 * layout/composition level; the component itself just emits the markup.
 */
const EvidenceLink: React.FC<EvidenceLinkProps> = ({ id, children }) => (
  <span id={id} data-chrome-zone="true">
    {children}
  </span>
);

export default EvidenceLink;
