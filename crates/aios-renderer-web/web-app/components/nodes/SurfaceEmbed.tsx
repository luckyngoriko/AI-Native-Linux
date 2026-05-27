import React from "react";

export interface SurfaceEmbedProps {
  id: string;
}

const SurfaceEmbed: React.FC<SurfaceEmbedProps> = ({ id }) => (
  <iframe id={id} title="embedded-surface" />
);

export default SurfaceEmbed;
