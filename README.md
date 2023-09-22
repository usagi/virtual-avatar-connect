## 各種連携の設定方法

### COEIROINK

1. API の docs を確認: <http://127.0.0.1:50032/docs#/default/speakers_v1_speakers_get>
2. 使用したいキャラクターのUUIDを確認: <http://127.0.0.1:50032/docs#/default/speakers_v1_speakers_get>
   - VACの補助機能: `virtual-avatar-creator --coeiroink-speakers`
3. VACの設定を行います:

```toml
[api.coeiroink]
# 設定すると連携が有効になります。未設定の場合は連携が無効になります。
#  v1/predict または v1/synthesis に対応しています。
url = "http://localhost:50032/v1/predict"
# 以下のパラメーターは省略するとデフォルト値が使用されます。
# v1/predict のパラメーター
speaker_uuid = "3c37646f-3881-5374-2a83-149267990abc"
style_id = 0
speedScale = 1
# v1/synthesis のパラメーター
volumeScale = 1
pitchScale = 0.5
intonationScale = 1
prePhonemeLength = 0.1
postPhonemeLength = 0.1
outputSamplingRate = 48000
```
