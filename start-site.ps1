cd "$PSScriptRoot/site"
npm run build
cd "$PSScriptRoot/dist"
npx serve .
