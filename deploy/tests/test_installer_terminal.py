"""Real controlling-terminal regression tests; no packages/services are changed."""
import errno
import importlib.util
import os
from pathlib import Path
import pty
import select
import signal
import time
import unittest
from unittest.mock import patch

path = Path(__file__).resolve().parents[1] / 'installer.py'
spec = importlib.util.spec_from_file_location('terminal_installer', path)
I = importlib.util.module_from_spec(spec)
spec.loader.exec_module(I)

class TerminalTests(unittest.TestCase):
    def test_missing_controlling_terminal_reports_interactive_and_unattended_options(self):
        with patch('builtins.open', side_effect=OSError(errno.ENXIO, 'no controlling terminal')):
            with self.assertRaises(I.InstallError) as error:
                I.terminal()
        self.assertIn('ssh -t', str(error.exception))
        self.assertIn('--config', str(error.exception))

    def drive(self, child, prompts, redirected_stdin=False):
        pid, fd = pty.fork()
        if pid == 0:
            try:
                if redirected_stdin:
                    null = os.open('/dev/null', os.O_RDONLY)
                    os.dup2(null, 0); os.close(null)
                child()
                os.write(1, b'\nTERMINAL_TEST_OK\n')
                os._exit(0)
            except BaseException:
                os.write(1, b'\nTERMINAL_TEST_FAILED\n')
                os._exit(1)
        data = b''; index = 0; cursor = 0; deadline = time.monotonic() + 15
        try:
            while time.monotonic() < deadline:
                if not select.select([fd], [], [], .1)[0]:
                    continue
                try: chunk = os.read(fd, 4096)
                except OSError as exc:
                    if exc.errno == errno.EIO: break
                    raise
                if not chunk: break
                data += chunk
                if index < len(prompts):
                    prompt, answer = prompts[index]
                    found = data.find(prompt.encode(), cursor)
                    if found >= 0:
                        cursor = found + len(prompt.encode()); index += 1
                        os.write(fd, (answer + '\n').encode())
                if b'TERMINAL_TEST_OK' in data or b'TERMINAL_TEST_FAILED' in data: break
            self.assertIn(b'TERMINAL_TEST_OK', data)
            self.assertEqual(index, len(prompts))
            return data.decode(errors='replace')
        finally:
            try: os.kill(pid, signal.SIGTERM)
            except ProcessLookupError: pass
            os.waitpid(pid, 0); os.close(fd)

    def test_complete_wizard_works_with_terminal_and_redirected_stdin(self):
        password = 'A-test-only-password-123'
        prompts = [('Домен панели: ', 'panel.example.com'),
                   ('Домен подписок: ', 'sub.example.com'),
                   ('Название вашего сервиса: ', 'Тестовый VPN'),
                   ('Валюта проекта [USD]: ', ''),
                   ('Логин владельца [admin]: ', ''),
                   ('Пароль владельца (Enter — создать надёжный): ', password),
                   ('Повторите пароль: ', password),
                   ('Токен Telegram-бота (Enter — настроить позже): ', '')]
        def child():
            config = I.wizard()
            assert config['panel_domain'] == 'panel.example.com'
            assert config['brand'] == 'Тестовый VPN'
            assert config['admin_password'] == password
        for redirected in (False, True):
            with self.subTest(redirected=redirected):
                output = self.drive(child, prompts, redirected)
                self.assertNotIn(password, output)

    def test_shared_terminal_supports_password_recovery_prompts(self):
        password = 'Another-test-only-password'
        def child():
            with I.terminal() as tty:
                tty.write('Логин: '); tty.flush()
                assert tty.readline().strip() == 'admin'
                assert I.getpass.getpass('Пароль: ', stream=tty) == password
        output = self.drive(child, [('Логин: ', 'admin'), ('Пароль: ', password)])
        self.assertNotIn(password, output)
