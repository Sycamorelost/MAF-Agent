/**
 * 主页:仅品牌 logo + 预设文案。
 * 入口统一收口到左侧导航与顶栏「新建对话」,主页不再堆放重复入口。
 */
import logoUrl from '../assets/logo.png';

export default function HomePage() {
  return (
    <div className="stage home">
      <img className="home-logo-img" src={logoUrl} alt="MAF Agent" />
      <h2 className="greeting">MAF Agent</h2>
      <p className="home-copy">
        本地优先的桌面 AI 助手 —— 切换任何模型,记忆都跟着走。
        <br />
        对话、记忆、附件全部落盘在本机 data/ 目录,无账号体系,断网也能浏览历史。
        <br />
        点顶栏「新建对话」开始,或从左侧列表继续上一次聊天。
      </p>
    </div>
  );
}
