import { invoke } from "@tauri-apps/api/core";

export type Filters = {
  stacks: number[];
  matchups: string[];
  pot_types: string[];
  streets: string[];
  textures: string[];
};

export type ActionView = { index: number; label: string; kind: string; amount_bb: number };

export type DrillView = {
  spot_id: string;
  stack: number;
  matchup: string;
  pot_type: string;
  street: string;
  texture: string;
  board: string[];
  hero: string;
  villain: string;
  hero_in_position: boolean;
  preflop: string;
  line: string[];
  hand: string[];
  pot_bb: number;
  to_call_bb: number;
  hero_stack_bb: number;
  villain_stack_bb: number;
  actions: ActionView[];
  node_type: string;
};

export type ActionResult = { label: string; ev_bb: number; freq_hand: number; freq_range: number };

export type Feedback = {
  chosen: number;
  score: { best: number; ev_loss_bb: number; rare: boolean; mixed: boolean; verdict: string };
  verdict: string;
  actions: ActionResult[];
  hand_desc: { made: string; draw: string | null; bucket: string };
  equity: number;
  reason: string;
};

export type Meta = { spots: number; stacks: number[]; matchups: string[]; pot_types: string[]; textures: string[] };

export type Summary = {
  attempts: number;
  avg_loss_bb: number;
  best_or_fine_pct: number;
  by_category: { category: string; n: number; avg_loss_bb: number }[];
};

export const api = {
  meta: () => invoke<Meta>("meta"),
  newDrill: (filters: Filters, mode: string) => invoke<DrillView>("new_drill", { filters, mode }),
  answer: (index: number) => invoke<Feedback>("answer", { index }),
  summary: () => invoke<Summary>("summary"),
};
