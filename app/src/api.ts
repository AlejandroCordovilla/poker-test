import { invoke } from "@tauri-apps/api/core";

export type Filters = {
  stacks: number[];
  matchups: string[];
  pot_types: string[];
  streets: string[];
  textures: string[];
  hand_buckets: string[];
  close_only: boolean;
  positions: string[];
};

export type GameMode = "spot" | "street" | "hand";

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
  situation: string;
  mode: GameMode;
  decision: number;
  live_solved: boolean;
  hero_is_aggressor: boolean;
};

export type ActionResult = { label: string; kind: string; ev_bb: number; freq_hand: number; freq_range: number };

export type Feedback = {
  chosen: number;
  score: { best: number; target: number; ev_loss_bb: number; ev_loss_pct: number; freq_diff: number; rare: boolean; mixed: boolean; rng: number | null; verdict: string };
  verdict: string;
  actions: ActionResult[];
  hand_desc: { made: string; draw: string | null; bucket: string };
  equity: number;
  reason: string;
  continues: boolean;
};

export type HandResult = { summary: string; villain_hand: string[] | null; board: string[]; pot_bb: number; hero_won: boolean | null };
export type Step = { events: string[]; next: DrillView | null; result: HandResult | null };

export type LeakStat = {
  key: string; name: string; n: number; you: number; solver: number; estimate: number;
  ci_low: number; ci_high: number; confidence: number; flagged: boolean; bb_per_100: number; verdict: string;
};
export type LeakReport = { decisions: number; stats: LeakStat[]; top: LeakStat[]; opponents: string[] };
export type AttemptRow = {
  id: number; ts: number; source: string; spot_id: string; street: string; hand: string; board: string;
  chosen: string; best: string; ev_loss_bb: number; ev_loss_pct: number; verdict: string; can_redrill: boolean;
};

export type Meta = { spots: number; stacks: number[]; matchups: string[]; pot_types: string[]; textures: string[] };

export type Summary = {
  attempts: number;
  avg_loss_bb: number;
  avg_loss_pct: number;
  good_pct: number;
  by_category: { category: string; n: number; avg_loss_bb: number }[];
  by_day: { day: string; n: number; avg_loss_bb: number; good_pct: number }[];
};

export const api = {
  meta: () => invoke<Meta>("meta"),
  newDrill: (filters: Filters, mode: string, gameMode: GameMode) => invoke<DrillView>("new_drill", { filters, mode, gameMode }),
  answer: (index: number, rng: number | null) => invoke<Feedback>("answer", { index, rng }),
  continueHand: () => invoke<Step>("continue_hand"),
  summary: () => invoke<Summary>("summary"),
  leaks: (source: string | null, opponent: string | null) => invoke<LeakReport>("leaks", { source, opponent }),
  attempts: (stat: string | null, source: string | null, opponent: string | null) => invoke<AttemptRow[]>("attempts", { stat, source, opponent }),
  redrill: (id: number) => invoke<DrillView>("redrill", { id }),
};

export type Preview = { hands: number; with_cards: number; players: [string, number][]; hero: string | null; aliases: Record<string, string>; already: number };
export type ImportResult = { imported: number; skipped_existing: number; preflop_decisions: number; postflop_queued: number };
export type ImportProgress = { queued: number; done: number; failed: number; current: string | null; last_seconds: number };
export type HandRow = {
  hand_id: string; hand_no: number; played: string; hero_cards: string; board: string; hero_pos: string; villain: string;
  pot_type: string; status: string; note: string; decisions: number; ev_loss: number; net_bb: number;
};
export type DecisionRow = { street: string; situation: string; board: string; chosen: string; best: string; ev_loss_bb: number; ev_loss_pct: number; verdict: string };

export const importApi = {
  preview: (text: string) => invoke<Preview>("import_preview", { text }),
  run: (text: string, file: string, hero: string, aliases: Record<string, string>) => invoke<ImportResult>("import_run", { text, file, hero, aliases }),
  status: () => invoke<ImportProgress>("import_status"),
  hands: () => invoke<HandRow[]>("import_hands"),
  detail: (handId: string) => invoke<DecisionRow[]>("import_hand_detail", { handId }),
};

export type ClassInfo = { class: number; name: string; weight: number; freq: number[]; ev: number[] };
export type RangeTask = { title: string; description: string; actions: string[]; kinds: string[]; pot_bb: number; board: string[]; classes: ClassInfo[] };
export const rangeApi = {
  task: (args: { kind: "preflop" | "flop"; table?: number; stack: number; scenario?: string; matchup?: string; potType?: string; who?: string }) =>
    invoke<RangeTask>("range_task", args),
};
