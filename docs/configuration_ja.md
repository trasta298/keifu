# 設定

keifu は `~/.config/keifu/config.toml` で設定できます。すべての設定は任意です。

## 自動更新

デフォルトでは、keifu は 10 秒ごとにコミットグラフを更新し、60 秒ごとに origin から fetch します。

```toml
[refresh]
# ローカル状態の自動更新を有効にする（デフォルト: true）
auto_refresh = true

# ローカル更新の間隔（秒）（デフォルト: 10、最小: 1）
refresh_interval = 10

# origin からの自動 fetch を有効にする（デフォルト: true）
auto_fetch = true

# リモート fetch の間隔（秒）（デフォルト: 60、最小: 10）
fetch_interval = 60
```

## グラフ表示

デフォルトでは、keifu はリモートブランチと、リモートブランチからのみ到達可能なコミットを表示します。
初期状態で非表示にしたい場合は、次のように設定できます。

```toml
[graph]
# リモートブランチをデフォルトで表示する（デフォルト: true）
show_remote_branches = false
```

TUI 上では `o` キーでリモートブランチ表示を切り替えられます。

### オプション一覧

| キー | 型 | デフォルト | 説明 |
| --- | --- | --- | --- |
| `auto_refresh` | bool | `true` | ローカル状態（コミット、ブランチ、ワーキングツリー）の自動更新を有効にする |
| `refresh_interval` | integer | `10` | ローカル更新の間隔（秒）（最小: 1） |
| `auto_fetch` | bool | `true` | origin からの自動 fetch を有効にする |
| `fetch_interval` | integer | `60` | リモート fetch の間隔（秒）（最小: 10） |
| `graph.show_remote_branches` | bool | `true` | リモートブランチと、リモートブランチからのみ到達可能なコミットを表示する |

### 自動更新を無効にする

自動更新を完全に無効にするには:

```toml
[refresh]
auto_refresh = false
auto_fetch = false
```

手動での更新は `R` キー、fetch は `f` キーで引き続き可能です。
