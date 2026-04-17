cd "$PSScriptRoot/wasm"
wasm-pack build --target web --out-dir ../site/wasm

cd "$PSScriptRoot"
npm run build
cd "$PSScriptRoot/dist"
npx serve .
