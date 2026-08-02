import { describe, expect, it } from "vitest";
import { parseDotenv } from "./dotenv";

describe("parseDotenv", () => {
  it("parses basic KEY=value lines", () => {
    const r = parseDotenv("A=1\nDB_URL=postgres://x\n");
    expect(r.vars).toEqual({ A: "1", DB_URL: "postgres://x" });
    expect(r.warnings).toEqual([]);
  });

  it("strips export prefix", () => {
    expect(parseDotenv("export FOO=bar").vars).toEqual({ FOO: "bar" });
  });

  it("skips comments and blank lines", () => {
    const r = parseDotenv("# top\n\n  \nA=1\n");
    expect(r.vars).toEqual({ A: "1" });
    expect(r.warnings).toEqual([]);
  });

  it("truncates unquoted values at an inline comment", () => {
    expect(parseDotenv("A=hello # world").vars).toEqual({ A: "hello" });
  });

  it("keeps # inside quoted values", () => {
    expect(parseDotenv('A="hello # world"').vars).toEqual({ A: "hello # world" });
  });

  it("single quotes are literal (no escapes)", () => {
    expect(parseDotenv("A='line\\nnot-newline'").vars).toEqual({ A: "line\\nnot-newline" });
  });

  it("double quotes support escapes", () => {
    expect(parseDotenv('A="a\\nb\\tc\\"d\\\\e"').vars).toEqual({ A: 'a\nb\tc"d\\e' });
  });

  it("trims whitespace around unquoted values", () => {
    expect(parseDotenv("A=  padded  ").vars).toEqual({ A: "padded" });
  });

  it("later duplicate keys win", () => {
    expect(parseDotenv("A=1\nA=2").vars).toEqual({ A: "2" });
  });

  it("warns on a line without '='", () => {
    const r = parseDotenv("JUSTAWORD");
    expect(r.vars).toEqual({});
    expect(r.warnings).toEqual([{ line: 1, text: "JUSTAWORD", reason: "missing '='" }]);
  });

  it("warns on an invalid key", () => {
    const r = parseDotenv("2BAD=x\nGOOD=y");
    expect(r.vars).toEqual({ GOOD: "y" });
    expect(r.warnings[0]).toMatchObject({ line: 1, reason: "invalid key" });
  });

  it("warns on an unterminated quote", () => {
    const r = parseDotenv('A="oops');
    expect(r.vars).toEqual({});
    expect(r.warnings[0]).toMatchObject({ line: 1, reason: "unterminated quote" });
  });

  it("empty and comment-only input yields nothing", () => {
    expect(parseDotenv("")).toEqual({ vars: {}, warnings: [] });
    expect(parseDotenv("# only\n# comments\n")).toEqual({ vars: {}, warnings: [] });
  });

  it("allows an empty value", () => {
    expect(parseDotenv("A=").vars).toEqual({ A: "" });
  });
});
