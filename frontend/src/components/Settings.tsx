import { useTranslation } from "react-i18next";
import { useEffect } from "react";
import { useAppDispatch, useAppSelector } from "../store/store";
import { ItemBox, ItemBoxContainer } from "./common/ItemBox";
import {
  Badge,
  Button,
  Flex,
  Input,
  InputNumber,
  Select,
  Slider,
  Space,
  Switch,
  Typography,
  Checkbox,
} from "antd";
import {
  forceSetLocalConfig,
  setAdbPath,
  setControllerPort,
  setVerticalPosition,
  setverticalMaskHeight,
  setWebPort,
  sethorizontalMaskWidth,
  setHorizontalPosition,
  setMappingLabelOpacity,
  setClipboardSync,
  setLanguage,
  setVideoCodec,
  setVideoBitRate,
  setVideoMaxSize,
  setVideoMaxFps,
  setVideoIFrameInterval,
  setAlwaysOnTop,
  setPresentMode,
  setVideoCodecOptions,
  setVideoLowLatency,
  setVideoRealtimePriority,
  setVideoQcomLowLatency,
  setVideoIntraRefresh,
  setShowDiagnostics,
  setHwDecode,
  setTheme,
} from "../store/localConfig";
import {
  setIsLoading,
  setShowUpdateDialog,
  setUpdateInfo,
} from "../store/other";
import { requestGet } from "../utils";
import i18n from "../i18n";
import { useMessageContext } from "../hooks";
import {
  BilibiliFilled,
  CloudSyncOutlined,
  GithubFilled,
  InfoCircleOutlined,
  SyncOutlined,
} from "@ant-design/icons";

const languageOptions = [
  {
    label: "简体中文",
    value: "zh-CN",
  },
  {
    label: "English",
    value: "en-US",
  },
  {
    label: "Türkçe",
    value: "tr-TR",
  },
];

const videoCodecOptions = ["H264", "H265", "AV1"].map((v) => ({
  value: v,
  label: v,
}));

export default function Settings() {
  const { t } = useTranslation();
  const dispatch = useAppDispatch();
  const messageApi = useMessageContext();
  const localConfig = useAppSelector((state) => state.localConfig);
  const updateInfo = useAppSelector((state) => state.other.updateInfo);

  async function loadLocalConfig() {
    dispatch(setIsLoading(true));
    try {
      const res = await requestGet("/api/config/get_config");
      dispatch(forceSetLocalConfig(res.data));
      i18n.changeLanguage(res.data.language);
    } catch (err: any) {
      messageApi?.error(err);
    }
    dispatch(setIsLoading(false));
  }

  useEffect(() => {
    loadLocalConfig();
  }, []);

  async function openDataPath() {
    dispatch(setIsLoading(true));
    try {
      const res = await requestGet("/api/config/open_data_path");
      messageApi?.success(res.message);
    } catch (err: any) {
      messageApi?.error(err);
    }
    dispatch(setIsLoading(false));
  }

  async function checkUpdate() {
    try {
      const res = await requestGet("/api/config/check_update");
      dispatch(
        setUpdateInfo({
          currentVersion: res.data.current_version,
          hasUpdate: res.data.has_update,
          latestVersion: res.data.latest_version,
          title: res.data.title,
          body: res.data.body,
          time: res.data.time,
        })
      );
      if (res.data.has_update) {
        dispatch(setShowUpdateDialog(true));
      }
    } catch (err: any) {
      messageApi?.error(err);
    }
  }

  return (
    <div className="page-container">
      <section>
        <Flex align="start" justify="space-between">
          <h2 className="title-with-line" style={{ marginBottom: 0 }}>
            {t("settings.title.header")}
          </h2>
          <Button
            type="primary"
            icon={<SyncOutlined />}
            shape="circle"
            onClick={loadLocalConfig}
          />
        </Flex>
        <h3 className="title-with-line-sub">{t("settings.title.basic")}</h3>
        <ItemBoxContainer className="mb-6">
          <ItemBox label={t("settings.language")}>
            <Select
              className="w-sm"
              value={localConfig.language}
              options={languageOptions}
              onChange={(v) => dispatch(setLanguage(v))}
            />
          </ItemBox>
          <ItemBox label={t("settings.theme")}>
            <Select
              className="w-sm"
              value={localConfig.theme}
              options={[
                { label: t("settings.themeOptions.dark"), value: "dark" },
                { label: t("settings.themeOptions.light"), value: "light" },
              ]}
              onChange={(v) => dispatch(setTheme(v))}
            />
          </ItemBox>
          <ItemBox label={t("settings.adbPath")}>
            <Input
              className="w-sm"
              value={localConfig.adbPath}
              onChange={(e) => dispatch(setAdbPath(e.target.value))}
            />
          </ItemBox>
          <ItemBox label={t("settings.clipboardSync")}>
            <Switch
              checked={localConfig.clipboardSync}
              onChange={(v) => dispatch(setClipboardSync(v))}
            />
          </ItemBox>
        </ItemBoxContainer>
        <h3 className="title-with-line-sub">{t("settings.title.mask")}</h3>
        <ItemBoxContainer className="mb-6">
          <ItemBox label={t("settings.alwaysOnTop")}>
            <Switch
              checked={localConfig.alwaysOnTop}
              onChange={(v) => dispatch(setAlwaysOnTop(v))}
            />
          </ItemBox>
          <ItemBox label={t("settings.mappingLabelOpacity")}>
            <Slider
              className="w-sm"
              min={0}
              max={1}
              step={0.01}
              onChange={(v) => dispatch(setMappingLabelOpacity(v))}
              value={localConfig.mappingLabelOpacity}
            />
          </ItemBox>
          <ItemBox label={t("settings.horizontalMaskWidth")}>
            <InputNumber
              className="w-sm"
              controls={false}
              min={50}
              value={localConfig.horizontalMaskWidth}
              onChange={(v) =>
                v !== null && dispatch(sethorizontalMaskWidth(v))
              }
            />
          </ItemBox>
          <ItemBox label={t("settings.horizontalMaskPosition")}>
            <Space.Compact className="w-sm">
              <InputNumber
                prefix="X:"
                className="w-50%"
                controls={false}
                value={localConfig.horizontalPosition[0]}
                onChange={(v) =>
                  v !== null &&
                  dispatch(
                    setHorizontalPosition([
                      v,
                      localConfig.horizontalPosition[1],
                    ])
                  )
                }
              />
              <InputNumber
                prefix="Y:"
                className="w-50%"
                controls={false}
                value={localConfig.horizontalPosition[1]}
                onChange={(v) =>
                  v !== null &&
                  dispatch(
                    setHorizontalPosition([
                      localConfig.horizontalPosition[0],
                      v,
                    ])
                  )
                }
              />
            </Space.Compact>
          </ItemBox>
          <ItemBox label={t("settings.verticalMaskHeight")}>
            <InputNumber
              className="w-sm"
              controls={false}
              min={50}
              value={localConfig.verticalMaskHeight}
              onChange={(v) => v !== null && dispatch(setverticalMaskHeight(v))}
            />
          </ItemBox>
          <ItemBox label={t("settings.verticalMaskPosition")}>
            <Space.Compact className="w-sm">
              <InputNumber
                prefix="X:"
                className="w-50%"
                controls={false}
                value={localConfig.verticalPosition[0]}
                onChange={(v) =>
                  v !== null &&
                  dispatch(
                    setVerticalPosition([v, localConfig.verticalPosition[1]])
                  )
                }
              />
              <InputNumber
                prefix="Y:"
                className="w-50%"
                controls={false}
                value={localConfig.verticalPosition[1]}
                onChange={(v) =>
                  v !== null &&
                  dispatch(
                    setVerticalPosition([localConfig.verticalPosition[0], v])
                  )
                }
              />
            </Space.Compact>
          </ItemBox>
        </ItemBoxContainer>
        <h3 className="title-with-line-sub">{t("settings.title.video")}</h3>
        <ItemBoxContainer className="mb-6">
          <ItemBox label={t("settings.videoCodec")}>
            <Select
              className="w-sm"
              value={localConfig.videoCodec}
              options={videoCodecOptions}
              onChange={(v) => dispatch(setVideoCodec(v))}
            />
          </ItemBox>
          <ItemBox label={t("settings.videoBitRate")}>
            <InputNumber
              className="w-sm"
              controls={false}
              min={1000000}
              suffix="bps"
              value={localConfig.videoBitRate}
              onChange={(v) => v !== null && dispatch(setVideoBitRate(v))}
            />
          </ItemBox>
          <ItemBox
            label={t("settings.videoMaxSize")}
            tooltip={t("settings.zeroUnlimitedTip")}
          >
            <InputNumber
              className="w-sm"
              controls={false}
              min={0}
              value={localConfig.videoMaxSize}
              onChange={(v) => v !== null && dispatch(setVideoMaxSize(v))}
            />
          </ItemBox>
          <ItemBox
            label={t("settings.videoMaxFps")}
            tooltip={t("settings.zeroUnlimitedTip")}
          >
            <InputNumber
              className="w-sm"
              controls={false}
              min={0}
              value={localConfig.videoMaxFps}
              onChange={(v) => v !== null && dispatch(setVideoMaxFps(v))}
            />
          </ItemBox>
          <ItemBox
            label={t("settings.videoIFrameInterval")}
            tooltip={t("settings.videoIFrameIntervalTip")}
          >
            <InputNumber
              className="w-sm"
              controls={false}
              min={1}
              value={localConfig.videoIFrameInterval}
              onChange={(v) => v !== null && dispatch(setVideoIFrameInterval(v))}
            />
          </ItemBox>
          <ItemBox
            label={t("settings.presentMode")}
            tooltip={t("settings.presentModeTip")}
          >
            <Select
              className="w-sm"
              value={localConfig.presentMode}
              options={[
                { value: "AutoVsync", label: t("settings.presentModeOptions.AutoVsync") },
                { value: "AutoNoVsync", label: t("settings.presentModeOptions.AutoNoVsync") },
                { value: "Immediate", label: t("settings.presentModeOptions.Immediate") },
                { value: "Mailbox", label: t("settings.presentModeOptions.Mailbox") },
              ]}
              onChange={(v) => dispatch(setPresentMode(v))}
            />
          </ItemBox>
          <ItemBox
            label={t("settings.videoCodecOptions")}
            tooltip={t("settings.videoCodecOptionsTip")}
          >
            <Input
              className="w-sm"
              value={localConfig.videoCodecOptions}
              onChange={(e) => dispatch(setVideoCodecOptions(e.target.value))}
              placeholder="e.g. latency=0,priority=0"
            />
          </ItemBox>
          <ItemBox label={t("settings.lowLatencyParams")}>
            <Space direction="vertical">
              <Checkbox
                checked={localConfig.videoLowLatency}
                onChange={(e) => dispatch(setVideoLowLatency(e.target.checked))}
              >
                {t("settings.lowLatencyParamsOptions.lowLatency")} (latency=0)
              </Checkbox>
              <Checkbox
                checked={localConfig.videoRealtimePriority}
                onChange={(e) => dispatch(setVideoRealtimePriority(e.target.checked))}
              >
                {t("settings.lowLatencyParamsOptions.realtimePriority")} (priority=0)
              </Checkbox>
              <Checkbox
                checked={localConfig.videoQcomLowLatency}
                onChange={(e) => dispatch(setVideoQcomLowLatency(e.target.checked))}
              >
                {t("settings.lowLatencyParamsOptions.qcomLowLatency")} (Qualcomm low-latency)
              </Checkbox>
              <Checkbox
                checked={localConfig.videoIntraRefresh}
                onChange={(e) => dispatch(setVideoIntraRefresh(e.target.checked))}
              >
                {t("settings.lowLatencyParamsOptions.intraRefresh")} (intra-refresh-period=60)
              </Checkbox>
            </Space>
          </ItemBox>
          <ItemBox label={t("settings.showDiagnostics")}>
            <Switch
              checked={localConfig.showDiagnostics}
              onChange={(v) => dispatch(setShowDiagnostics(v))}
            />
          </ItemBox>
          <ItemBox label={t("settings.hwDecode")}>
            <Switch
              checked={localConfig.hwDecode}
              onChange={(v) => dispatch(setHwDecode(v))}
            />
          </ItemBox>
        </ItemBoxContainer>

        <h3 className="title-with-line-sub">{t("settings.title.advance")}</h3>
        <ItemBoxContainer className="mb-6">
          <ItemBox label={t("settings.webPort")}>
            <InputNumber
              className="w-sm"
              controls={false}
              value={localConfig.webPort}
              onChange={(v) => v !== null && dispatch(setWebPort(v))}
            />
          </ItemBox>
          <ItemBox label={t("settings.controllerPort")}>
            <InputNumber
              className="w-sm"
              controls={false}
              value={localConfig.controllerPort}
              onChange={(v) => v !== null && dispatch(setControllerPort(v))}
            />
          </ItemBox>
          <ItemBox>
            <Button type="primary" onClick={openDataPath}>
              {t("settings.openDataPath")}
            </Button>
          </ItemBox>
        </ItemBoxContainer>
      </section>
      <section>
        <h2 className="title-with-line">{t("settings.about.title")}</h2>
        <Typography.Paragraph>{t("settings.about.intro")}</Typography.Paragraph>
        <Flex gap="large">
          <Button
            type="text"
            icon={<GithubFilled />}
            onClick={() =>
              window.open("https://github.com/AkiChase/scrcpy-mask", "_blank")
            }
          >
            Github
          </Button>
          <Button
            type="text"
            icon={<BilibiliFilled />}
            onClick={() =>
              window.open("https://space.bilibili.com/440760180", "_blank")
            }
          >
            BiliBili
          </Button>
        </Flex>
        <Flex gap="large" align="center" className="mt-4">
          <Button
            type="primary"
            icon={<CloudSyncOutlined />}
            onClick={checkUpdate}
          >
            {t("settings.about.checkUpdate")}
          </Button>
          <Badge dot={updateInfo.hasUpdate}>
            <Button
              type="primary"
              icon={<InfoCircleOutlined />}
              onClick={() => dispatch(setShowUpdateDialog(true))}
            >
              {t("settings.about.showUpdateDialog")}
            </Button>
          </Badge>
        </Flex>
        <Flex gap="large" align="center" className="mt-4">
          <Typography.Text>
            {t("settings.about.currentVersion")}: {updateInfo.currentVersion}
          </Typography.Text>
          <Typography.Text>
            {t("settings.about.latestVersion")}: {updateInfo.latestVersion}
          </Typography.Text>
        </Flex>
      </section>
    </div>
  );
}
