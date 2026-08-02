import { common } from "./common";
import { skills } from "./skills";
import { skillDetail } from "./skill-detail";
import { routePage } from "./route";
import { configCenter } from "./config-center";
import { misc } from "./misc";

export interface Entry {
  zh: string;
  en: string;
}

export const dicts: Record<string, Entry> = {
  ...common,
  ...skills,
  ...skillDetail,
  ...routePage,
  ...configCenter,
  ...misc,
};
