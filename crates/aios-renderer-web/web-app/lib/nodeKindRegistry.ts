import type { ComponentType } from "react";
import * as Nodes from "@/components/nodes";

/**
 * Closed union mirroring the Rust NodeKind enum (S7.2).
 * Every variant has a corresponding React component in components/nodes/.
 */
export type NodeKind =
  | "Container"
  | "Divider"
  | "Spacer"
  | "Text"
  | "Heading"
  | "InlineCode"
  | "CodeBlock"
  | "Card"
  | "List"
  | "Table"
  | "Form"
  | "ActionButton"
  | "Visualization"
  | "Stream"
  | "SurfaceEmbed"
  | "SecurityIndicator"
  | "ApprovalPrompt"
  | "EvidenceLink"
  | "AgentMessage";

export const NODE_KIND_VARIANTS: readonly NodeKind[] = [
  "Container",
  "Divider",
  "Spacer",
  "Text",
  "Heading",
  "InlineCode",
  "CodeBlock",
  "Card",
  "List",
  "Table",
  "Form",
  "ActionButton",
  "Visualization",
  "Stream",
  "SurfaceEmbed",
  "SecurityIndicator",
  "ApprovalPrompt",
  "EvidenceLink",
  "AgentMessage",
] as const;

/**
 * Type-checked registry mapping every NodeKind variant to its React component.
 * Inserting a new variant without adding the component is a compile error.
 */
export const nodeKindRegistry: Record<
  NodeKind,
  ComponentType<{ id: string; children?: React.ReactNode }>
> = {
  Container: Nodes.Container,
  Divider: Nodes.Divider,
  Spacer: Nodes.Spacer,
  Text: Nodes.Text,
  Heading: Nodes.Heading,
  InlineCode: Nodes.InlineCode,
  CodeBlock: Nodes.CodeBlock,
  Card: Nodes.Card,
  List: Nodes.List,
  Table: Nodes.Table,
  Form: Nodes.Form,
  ActionButton: Nodes.ActionButton,
  Visualization: Nodes.Visualization,
  Stream: Nodes.Stream,
  SurfaceEmbed: Nodes.SurfaceEmbed,
  SecurityIndicator: Nodes.SecurityIndicator,
  ApprovalPrompt: Nodes.ApprovalPrompt,
  EvidenceLink: Nodes.EvidenceLink,
  AgentMessage: Nodes.AgentMessage,
};
