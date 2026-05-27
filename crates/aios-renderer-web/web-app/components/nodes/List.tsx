import React from "react";

export interface ListProps {
  id: string;
  children?: React.ReactNode;
}

const List: React.FC<ListProps> = ({ id, children }) => (
  <ul id={id}>{children}</ul>
);

export default List;
