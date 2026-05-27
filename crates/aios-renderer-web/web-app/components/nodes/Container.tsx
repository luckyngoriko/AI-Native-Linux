import React from "react";

export interface ContainerProps {
  id: string;
  children?: React.ReactNode;
}

const Container: React.FC<ContainerProps> = ({ id, children }) => (
  <div id={id} style={{ display: "flex" }}>
    {children}
  </div>
);

export default Container;
