import os
import re
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SKILL = ROOT / "skills" / "binus-cli" / "SKILL.md"
README = ROOT / "README.md"
BINUS_NAME = "binus.exe" if os.name == "nt" else "binus"
BINUS = ROOT / "spikes" / "001-auth" / "target" / "release" / BINUS_NAME
PERSONAL_PATH_RE = re.compile(r"(?i)(/home/|/Users/|C:\\Users\\|/root/)")


class AgentSkillAcceptance(unittest.TestCase):
    def test_skill_and_readme_link_exist(self):
        self.assertTrue(SKILL.is_file(), "portable agent skill is missing")
        readme = README.read_text(encoding="utf-8")
        self.assertRegex(readme, r"\]\(skills/binus-cli/SKILL\.md\)")

    def test_frontmatter_and_portability(self):
        text = SKILL.read_text(encoding="utf-8")
        self.assertFalse(text.startswith("\ufeff"), "skill must not have a BOM")
        frontmatter = re.match(r"\A---\n(.*?)\n---\n", text, re.S)
        self.assertIsNotNone(frontmatter, "YAML frontmatter is required")
        self.assertRegex(frontmatter.group(1), r"(?m)^name: binus-cli$")
        description = re.search(r'(?m)^description: "([^"]*)"$', frontmatter.group(1))
        self.assertIsNotNone(description)
        self.assertLessEqual(len(description.group(1)), 60)
        self.assertTrue(description.group(1).endswith("."))
        self.assertLessEqual(len(text.splitlines()), 200)
        self.assertNotRegex(text, PERSONAL_PATH_RE)
        for label, target in re.findall(r"\[([^\]]+)\]\(([^)]+)\)", text):
            self.assertFalse(target.startswith(("/", "~", "file:")), (label, target))
            self.assertTrue((SKILL.parent / target).exists(), (label, target))

    def test_personal_path_guard_matches_synthetic_paths(self):
        for path in (r"C:\Users\synthetic\project", "/home/synthetic/project"):
            with self.subTest(path=path):
                self.assertRegex(path, PERSONAL_PATH_RE)

    def test_documented_command_shapes_match_real_help(self):
        self.assertTrue(BINUS.is_file(), "prebuilt validation binary is missing")
        paths = [
            ["auth"], ["auth", "login"], ["auth", "status"], ["auth", "check"],
            ["auth", "logout"], ["courses"], ["courses", "periods"],
            ["courses", "list"], ["courses", "sessions"],
            ["courses", "sessions", "list"], ["courses", "sessions", "show"],
            ["forums", "threads"], ["forums", "threads", "list"],
            ["forums", "threads", "show"],
        ]
        for path in paths:
            with self.subTest(command=" ".join(path)):
                result = subprocess.run(
                    [str(BINUS), *path, "--help"], capture_output=True, text=True,
                    check=False, timeout=10,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("Usage:", result.stdout)
                required = {
                    ("courses", "list"): "--period <PERIOD>",
                    ("courses", "sessions", "list"): "--class <CLASS>",
                    ("forums", "threads", "list"): "--session <SESSION>",
                    ("forums", "threads", "show"): "--class <CLASS>",
                }.get(tuple(path))
                if required:
                    self.assertIn(required, result.stdout)
        skill = SKILL.read_text(encoding="utf-8")
        for token in (
            "auth login", "auth status", "auth check", "auth logout",
            "courses periods", "courses list --period", "courses sessions list --class",
            "courses sessions show", "forums threads list --class", "--session",
            "forums threads show",
        ):
            self.assertIn(token, skill)


if __name__ == "__main__":
    unittest.main()
