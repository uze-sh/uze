import { readFile } from "node:fs/promises";

export interface User {
  id: number;
  name: string;
}

export async function load(path: string): Promise<User[]> {
  const text = await readFile(path, "utf8");
  return JSON.parse(text) as User[];
}
