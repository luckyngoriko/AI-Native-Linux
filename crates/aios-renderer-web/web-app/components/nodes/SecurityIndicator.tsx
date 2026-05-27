import React from "react";

export interface SecurityIndicatorProps {
  id: string;
}

/**
 * chrome-zone: closed shadow root rendering happens at the
 * layout/composition level; the component itself just emits the markup.
 */
const SecurityIndicator: React.FC<SecurityIndicatorProps> = ({ id }) => (
  <div id={id} data-chrome-zone="true" />
);

export default SecurityIndicator;
