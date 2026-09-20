#!/usr/bin/env python3
# Copyright (c) 2026 VinVel
# SPDX-License-Identifier: AGPL-3.0-only
"""Run the development timeline fixture in the system WebKitGTK browser.

Start pnpm dev first. Uses only Python's standard library and WebKitWebDriver.
Touch is dispatched as synthetic touch-list events because WebKitWebDriver does not implement
touch pointer actions; physical touch hardware remains a separate platform check.
"""
import argparse
import base64
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--url", default="http://localhost:1420/__timeline")
parser.add_argument("--port", type=int, default=4445)
parser.add_argument("--browser-binary", default="/usr/libexec/webkit2gtk-4.1/MiniBrowser")
parser.add_argument("--output", default="/tmp/hyperion-viewport-webkit.json")
args = parser.parse_args()
endpoint = f"http://127.0.0.1:{args.port}"


def request(method, path, payload=None):
    data = None if payload is None else json.dumps(payload).encode()
    req = urllib.request.Request(endpoint + path, data, {"Content-Type": "application/json"}, method=method)
    try:
        with urllib.request.urlopen(req, timeout=100) as response:
            return json.load(response)["value"]
    except urllib.error.HTTPError as error:
        raise RuntimeError(error.read().decode()) from error


with tempfile.TemporaryDirectory(prefix="hyperion-webkit-") as directory:
    environment = dict(os.environ, GSETTINGS_BACKEND="memory", XDG_CACHE_HOME=directory, GDK_BACKEND="x11")
    with open(Path(directory) / "driver.log", "w") as log:
        driver = subprocess.Popen(["WebKitWebDriver", f"--port={args.port}"], env=environment, stdout=log, stderr=log)
        session = None
        try:
            for attempt in range(50):
                try:
                    request("GET", "/status")
                    break
                except urllib.error.URLError:
                    time.sleep(0.1)
            session_info = request("POST", "/session", {"capabilities": {"alwaysMatch": {
                "webkitgtk:browserOptions": {"binary": args.browser_binary, "args": ["--automation"]}
            }}})
            session = session_info["sessionId"]
            prefix = "/session/" + session
            request("POST", prefix + "/timeouts", {"script": 90000})
            all_results = []
            for width, height in [(1100, 850), (390, 780)]:
                print("Running viewport", width, height, flush=True)
                request("POST", prefix + "/window/rect", {"width": width, "height": height})
                request("POST", prefix + "/url", {"url": args.url})
                result = request("POST", prefix + "/execute/async", {
                    "script": """const done = arguments[0];
                    async function run() {
                      const deadline = Date.now() + 10000;
                      while (!window.timelineFixture) {
                        if (Date.now() > deadline) {
                          // Surface module errors instead of waiting for the scenario timeout.
                          await import("/src/screens/app-shell/timeline/viewportFixture.tsx");
                          throw Error("The fixture did not mount: " + location.href);
                        }
                        await new Promise(r => setTimeout(r, 50));
                      }
                      const viewport = await window.timelineFixture.run();
                      const input = await window.timelineFixture.runInput();
                      const lifecycle = await window.timelineFixture.runLifecycle();
                      const failure = await window.timelineFixture.runFailure();
                      return [...viewport, ...input, ...lifecycle, ...failure];
                    }
                    run().then(done).catch(e => done({error: String(e), stack:e.stack}));""",
                    "args": []
                })
                if isinstance(result, dict):
                    raise RuntimeError(result)
                print(json.dumps(result), flush=True)
                screenshot = request("GET", prefix + "/screenshot")
                Path(args.output).with_suffix(f".{width}.png").write_bytes(base64.b64decode(screenshot))
                before = request("POST", prefix + "/execute/sync", {
                    "script": """const s=document.querySelector('.room-timeline-scroller');
                    s.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true}));
                    s.scrollTop=0; s.focus({preventScroll:true});
                    window.timelineFixture.configure(50,false);
                    return window.timelineFixture.status().requests;""", "args":[]
                })
                element = request("POST", prefix + "/element", {"using":"css selector","value":".room-timeline-scroller"})
                element_id = element["element-6066-11e4-a52e-4f735466cecf"]
                try:
                    request("POST", prefix + "/element/" + element_id + "/value", {"text":"\ue013"})
                except RuntimeError as error:
                    if "unsupported operation" not in str(error):
                        raise
                    native_keyboard = "Unavailable: WebKitWebDriver reports unsupported operation"
                else:
                    after = request("POST", prefix + "/execute/async", {
                        "script":"const done=arguments[0]; setTimeout(()=>done(window.timelineFixture.status().requests),500);", "args":[]
                    })
                    result.append({"name":"native WebDriver keyboard pagination", "pass":after==before+1})
                    native_keyboard = "Executed"
                actual_width = request("POST", prefix + "/execute/sync", {
                    "script":"return document.querySelector('.room-timeline-scroller').clientWidth;", "args":[]
                })
                all_results.append({"window": {"width": width, "height": height, "viewportWidth":actual_width}, "scenarios": result})
            report = {"browser": session_info["capabilities"], "nativeKeyboard": native_keyboard, "touch": "Synthetic touch-list events; WebDriver native touch is unsupported", "runs": all_results}
            Path(args.output).write_text(json.dumps(report, indent=2) + "\n")
            for run in all_results:
                for scenario in run["scenarios"]:
                    print(run["window"]["width"], "PASS" if scenario["pass"] else "FAIL", scenario)
            if not all(scenario["pass"] for run in all_results for scenario in run["scenarios"]):
                raise SystemExit(1)
        except Exception:
            if session:
                print(request("POST", "/session/" + session + "/execute/sync", {
                    "script":"return {phase:document.documentElement.dataset.fixturePhase, status:window.timelineFixture?.status(), text:document.body.innerText.slice(-250)};", "args":[]
                }), flush=True)
            raise
        finally:
            if session:
                request("DELETE", "/session/" + session)
            driver.terminate()
            driver.wait(timeout=10)
