import { describe, expect, it } from "vitest";
import { parseModelPackage } from "./petModel";

function modelFile(path: string, contents: string) {
  const file = new File([contents], path.split("/").pop()!);
  Object.defineProperty(file, "webkitRelativePath", { value: path });
  return file;
}

function descriptor(moc: string) {
  return modelFile("Pet/skin.model3.json", JSON.stringify({
    Version: 3,
    FileReferences: { Moc: moc, Textures: ["textures/skin.png"] },
  }));
}

describe("local Cubism package", () => {
  it("stores only files declared by the chosen model", async () => {
    const model = await parseModelPackage([
      descriptor("skin.moc3"),
      modelFile("Pet/skin.moc3", "MOC3"),
      modelFile("Pet/textures/skin.png", "image"),
      modelFile("Pet/notes.txt", "notes"),
    ]);
    expect(model.name).toBe("skin");
    expect(model.files.map(file => file.reference)).toEqual(["skin.moc3", "textures/skin.png"]);
  });

  it("rejects external asset references and missing textures", async () => {
    await expect(parseModelPackage([descriptor("https://example.com/skin.moc3")])).rejects.toThrow("本地相对路径");
    await expect(parseModelPackage([descriptor("skin.moc3"), modelFile("Pet/skin.moc3", "MOC3")]))
      .rejects.toThrow("textures/skin.png");
  });
});
