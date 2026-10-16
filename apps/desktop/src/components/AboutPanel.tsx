import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { UpdateSettings } from "./UpdateSettings";

interface FfmpegInfo {
  available: boolean;
  bundleVersion: string | null;
  ffmpegVersion: string | null;
  ffmpegVersionLine: string | null;
  path: string | null;
  repository: string | null;
}

const CAPTO_REPO_URL = "https://github.com/ciryanna/Capto";
const CAPTO_LICENSE_URL = "https://github.com/ciryanna/Capto/blob/main/LICENSE";
const DEVELOPER_NAME = "Elwina Vardal";
const DEVELOPER_SITE = "https://www.elwina.work";
const DEVELOPER_GITHUB = "https://github.com/elwina";
const LICENSE = "MIT";

function repoUrl(slug: string | null | undefined): string | null {
  if (!slug) return null;
  if (/^https?:\/\//i.test(slug)) return slug;
  return `https://github.com/${slug}`;
}

function displayHost(url: string): string {
  return url.replace(/^https:\/\//, "");
}

function DevIcon({ kind }: { kind: "person" | "globe" | "github" }) {
  if (kind === "person") {
    return (
      <svg className="about-dev-icon" viewBox="0 0 24 24" aria-hidden>
        <path
          fill="currentColor"
          d="M12 12a4.5 4.5 0 1 0-4.5-4.5A4.5 4.5 0 0 0 12 12Zm0 2c-4 0-7.5 2-7.5 4.5V20h15v-1.5C19.5 16 16 14 12 14Z"
        />
      </svg>
    );
  }
  if (kind === "globe") {
    return (
      <svg className="about-dev-icon" viewBox="0 0 24 24" aria-hidden>
        <path
          fill="currentColor"
          d="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2Zm7.4 9h-3.1a13.6 13.6 0 0 0-1.3-5 8 8 0 0 1 4.4 5ZM12 4c.9 0 2.3 2.2 3 7H9c.7-4.8 2.1-7 3-7ZM4.6 13h3.1a13.6 13.6 0 0 0 1.3 5 8 8 0 0 1-4.4-5Zm3.1-2H4.6a8 8 0 0 1 4.4-5 13.6 13.6 0 0 0-1.3 5ZM12 20c-.9 0-2.3-2.2-3-7h6c-.7 4.8-2.1 7-3 7Zm3 0a8 8 0 0 0 4.4-5h-3.1a13.6 13.6 0 0 1-1.3 5Z"
        />
      </svg>
    );
  }
  return (
    <svg className="about-dev-icon" viewBox="0 0 24 24" aria-hidden>
      <path
        fill="currentColor"
        d="M12 2a10 10 0 0 0-3.2 19.5c.5.1.7-.2.7-.5v-1.7c-2.8.6-3.4-1.2-3.4-1.2-.5-1.1-1.1-1.4-1.1-1.4-.9-.6.1-.6.1-.6 1 .1 1.5 1 1.5 1 .9 1.5 2.3 1.1 2.9.8.1-.6.3-1.1.6-1.3-2.2-.3-4.6-1.1-4.6-5a3.9 3.9 0 0 1 1-2.7 3.6 3.6 0 0 1 .1-2.7s.8-.3 2.8 1a9.6 9.6 0 0 1 5 0c2-1.3 2.8-1 2.8-1a3.6 3.6 0 0 1 .1 2.7 3.9 3.9 0 0 1 1 2.7c0 3.9-2.3 4.7-4.6 5 .4.3.7.9.7 1.9v2.8c0 .3.2.6.7.5A10 10 0 0 0 12 2Z"
      />
    </svg>
  );
}

export function AboutPanel() {
  const { t } = useTranslation();
  const [version, setVersion] = useState<string>("…");
  const [ffmpeg, setFfmpeg] = useState<FfmpegInfo | null>(null);
  const [donateOk, setDonateOk] = useState(true);

  useEffect(() => {
    void getVersion()
      .then(setVersion)
      .catch(() => setVersion("?"));
    void invoke<FfmpegInfo>("get_ffmpeg_info")
      .then(setFfmpeg)
      .catch(() =>
        setFfmpeg({
          available: false,
          bundleVersion: null,
          ffmpegVersion: null,
          ffmpegVersionLine: null,
          path: null,
          repository: null,
        }),
      );
  }, []);

  const ffmpegRepo = repoUrl(ffmpeg?.repository ?? "ciryanna/capto-ffmpeg");

  return (
    <div className="about-stack">
      <section className="capto-card about-panel">
        <div className="about-hero-row">
          <div className="about-hero-text">
            <div className="about-product">{t("appName")}</div>
            <p className="about-tagline">{t("tagline")}</p>
          </div>
          <img
            className="about-logo"
            src="/capto-mark.png"
            alt={t("appName")}
            width={72}
            height={72}
            draggable={false}
          />
        </div>

        <div className="about-updates">
          <UpdateSettings />
        </div>

        <div className="about-block">
          <div className="card-label">{t("aboutProject")}</div>
          <dl className="about-dl">
            <div>
              <dt>{t("aboutAppVersion")}</dt>
              <dd>{version}</dd>
            </div>
            <div>
              <dt>{t("aboutLicense")}</dt>
              <dd>
                <button
                  type="button"
                  className="link-btn"
                  onClick={() => void openUrl(CAPTO_LICENSE_URL).catch(() => undefined)}
                >
                  {LICENSE}
                </button>
              </dd>
            </div>
            <div>
              <dt>{t("aboutSource")}</dt>
              <dd>
                <button
                  type="button"
                  className="link-btn"
                  onClick={() => void openUrl(CAPTO_REPO_URL).catch(() => undefined)}
                >
                  {displayHost(CAPTO_REPO_URL)}
                </button>
              </dd>
            </div>
          </dl>
        </div>

        <div className="about-block">
          <div className="card-label">{t("aboutFfmpeg")}</div>
          {!ffmpeg ? (
            <p className="about-muted">{t("aboutLoading")}</p>
          ) : (
            <dl className="about-dl">
              <div>
                <dt>{t("aboutCaptoFfmpegVersion")}</dt>
                <dd>{ffmpeg.bundleVersion ?? "—"}</dd>
              </div>
              <div>
                <dt>{t("aboutFfmpegVersion")}</dt>
                <dd title={ffmpeg.ffmpegVersionLine ?? undefined}>
                  {ffmpeg.available ? (ffmpeg.ffmpegVersion ?? "—") : t("aboutFfmpegMissingShort")}
                </dd>
              </div>
              <div>
                <dt>{t("aboutFfmpegPath")}</dt>
                <dd className="about-path" title={ffmpeg.path ?? undefined}>
                  {ffmpeg.path ?? "—"}
                </dd>
              </div>
              <div>
                <dt>{t("aboutSource")}</dt>
                <dd>
                  {ffmpegRepo ? (
                    <button
                      type="button"
                      className="link-btn"
                      onClick={() => void openUrl(ffmpegRepo).catch(() => undefined)}
                    >
                      {displayHost(ffmpegRepo)}
                    </button>
                  ) : (
                    "—"
                  )}
                </dd>
              </div>
            </dl>
          )}
        </div>
      </section>

      <section className="capto-card about-developer">
        <div className="card-label">{t("aboutDeveloper")}</div>
        <ul className="about-dev-list">
          <li className="about-dev-row">
            <DevIcon kind="person" />
            <span>{DEVELOPER_NAME}</span>
          </li>
          <li className="about-dev-row">
            <DevIcon kind="globe" />
            <button
              type="button"
              className="link-btn"
              onClick={() => void openUrl(DEVELOPER_SITE).catch(() => undefined)}
            >
              www.elwina.work
            </button>
          </li>
          <li className="about-dev-row">
            <DevIcon kind="github" />
            <button
              type="button"
              className="link-btn"
              onClick={() => void openUrl(DEVELOPER_GITHUB).catch(() => undefined)}
            >
              {displayHost(DEVELOPER_GITHUB)}
            </button>
          </li>
        </ul>

        <div className="about-donate">
          <div className="card-label">{t("aboutDonate")}</div>
          <p className="about-muted">{t("aboutDonateAlipay")}</p>
          {donateOk ? (
            <img
              className="about-donate-qr"
              src="/donate-alipay.png"
              alt={t("aboutDonateAlipay")}
              width={168}
              height={168}
              draggable={false}
              onError={() => setDonateOk(false)}
            />
          ) : (
            <p className="about-muted">{t("aboutDonateMissing")}</p>
          )}
        </div>
      </section>
    </div>
  );
}
