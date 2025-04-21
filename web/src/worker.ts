import { expose } from "comlink";
import * as module from "./worker_module";

expose(module);
