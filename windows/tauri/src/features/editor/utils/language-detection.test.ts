import { describe, expect, test } from "bun:test";
import { detectLanguageFromFileName, detectLanguageFromPath } from "./language-detection";

describe("editor language detection fallbacks", () => {
  test("recognizes Java sources even before extension contributions initialize", () => {
    expect(detectLanguageFromPath("C:\\work\\src\\DirectUploadService.JAVA")).toBe("java");
    expect(detectLanguageFromFileName("DirectUploadService.java")).toBe("java");
  });

  test("recognizes JVM family sources, Gradle scripts, and Jenkinsfiles", () => {
    expect(detectLanguageFromPath("C:\\work\\src\\User.kt")).toBe("kotlin");
    expect(detectLanguageFromPath("C:\\work\\settings.gradle.kts")).toBe("kotlin");
    expect(detectLanguageFromPath("C:\\work\\src\\Repo.scala")).toBe("scala");
    expect(detectLanguageFromPath("C:\\work\\script.sc")).toBe("scala");
    expect(detectLanguageFromPath("C:\\work\\build.gradle")).toBe("groovy");
    expect(detectLanguageFromPath("/work/deploy.gvy")).toBe("groovy");
    expect(detectLanguageFromFileName("Jenkinsfile")).toBe("groovy");
    expect(detectLanguageFromFileName("Jenkinsfile.release")).toBe("groovy");
  });
});
