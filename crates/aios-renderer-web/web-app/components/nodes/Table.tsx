import React from "react";

export interface TableProps {
  id: string;
  children?: React.ReactNode;
}

const Table: React.FC<TableProps> = ({ id, children }) => (
  <table id={id}>{children}</table>
);

export default Table;
