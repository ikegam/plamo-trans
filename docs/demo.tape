# Record the README demo with the real binary: vhs docs/demo.tape
# https://github.com/charmbracelet/vhs  (needs a CJK monospace font installed)
Output docs/demo.gif

Set Shell bash
Set FontFamily "Noto Sans Mono CJK JP"
Set FontSize 18
Set Width 900
Set Height 340
Set Padding 20
Set TypingSpeed 45ms
Set Theme "Catppuccin Mocha"

Hide
Type "export PS1='$ '" Enter
Type "clear" Enter
Show

Type "trans 吾輩は猫である。名前はまだ無い。"
Sleep 300ms
Enter
Sleep 2s

Type `echo "Please let me know if you have any questions." | trans -p`
Sleep 300ms
Enter
Sleep 2s

Type "trans -t zh-hans 今日はいい天気ですね"
Sleep 300ms
Enter
Sleep 2s

Type "time trans -ff 吾輩は猫である。"
Sleep 300ms
Enter
Sleep 3s
