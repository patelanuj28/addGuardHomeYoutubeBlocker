# PowerShell
cargo build --release
$env:ADGUARD_URL="http://your-adguard-ip:port"
$env:ADGUARD_USERNAME="your-username"
$env:ADGUARD_PASSWORD="your-password"

$env:MQTT_HOST="mqtt-broker-ip"
$env:MQTT_PORT="1883"
$env:MQTT_USERNAME="mqtt-username"
$env:MQTT_PASSWORD="mqtt-password"
$env:MQTT_TOPIC="/home/adGuardhomeyoutube/youtube"
.\target\release\adGuardHomeYoutube.exe