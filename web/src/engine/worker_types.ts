import { type Remote } from "comlink";

export type GameWorkerModule = typeof import("./worker_module");
export type GameWorker = Remote<GameWorkerModule>;
