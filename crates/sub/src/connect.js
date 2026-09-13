(function(){
  'use strict';
  var lang=document.documentElement.lang==='en'?'en':'ru';
  function text(ru,en){return lang==='en'?en:ru;}
  var languageSwitch=document.getElementById('languageSwitch');
  if(languageSwitch)languageSwitch.addEventListener('click',function(){
    var next=lang==='ru'?'en':'ru';
    document.cookie='sn.sub.lang='+next+'; Path=/; Max-Age=31536000; SameSite=Lax'+(location.protocol==='https:'?'; Secure':'');
    var url=new URL(location.href);url.searchParams.set('lang',next);location.assign(url.href);
  });
  var themeMode='auto',systemTheme=matchMedia('(prefers-color-scheme:dark)');
  try{themeMode=localStorage.getItem('sn.app.theme')||'auto';}catch(_){}
  if(!['auto','light','dark'].includes(themeMode))themeMode='auto';
  function syncTheme(){document.documentElement.dataset.theme=themeMode==='auto'?(systemTheme.matches?'dark':'light'):themeMode;document.querySelectorAll('[name="theme"]').forEach(function(input){input.checked=input.value===themeMode;});}
  document.querySelectorAll('[name="theme"]').forEach(function(input){input.addEventListener('change',function(){themeMode=input.value;try{localStorage.setItem('sn.app.theme',themeMode);}catch(_){}syncTheme();});});
  systemTheme.addEventListener('change',syncTheme);syncTheme();
  var select=document.getElementById('deviceSelect');
  var applications=Array.from(document.querySelectorAll('.application'));
  var picker=document.getElementById('appPicker');
  var choices=Array.from(document.querySelectorAll('[data-choice]'));
  var remembered={};
  var feedback=document.getElementById('feedback');
  var feedbackTimer;
  function notify(text){
    clearTimeout(feedbackTimer);
    feedback.textContent=text;feedback.hidden=false;
    feedbackTimer=setTimeout(function(){feedback.hidden=true;},3500);
  }
  function openSheet(id){
    var sheet=document.getElementById(id);
    if(!sheet)return;
    document.querySelectorAll('dialog[open]').forEach(function(d){if(d!==sheet)d.close();});
    if(!sheet.open)sheet.showModal();
  }
  document.querySelectorAll('[data-sheet]').forEach(function(button){button.addEventListener('click',function(){openSheet(button.dataset.sheet);});});
  document.querySelectorAll('dialog').forEach(function(sheet){
    sheet.querySelector('[data-close]').addEventListener('click',function(){sheet.close();});
    sheet.addEventListener('click',function(event){
      if(event.target!==sheet)return;
      var r=sheet.getBoundingClientRect();
      if(event.clientX<r.left||event.clientX>r.right||event.clientY<r.top||event.clientY>r.bottom)sheet.close();
    });
  });
  function chooseApp(id){
    var app=applications.find(function(a){return a.dataset.app===id;});
    var choice=choices.find(function(c){return c.dataset.choice===id;});
    if(!app||!choice||!picker)return;
    remembered[app.dataset.platform]=id;
    applications.forEach(function(a){a.hidden=a!==app;});
    choices.forEach(function(c){c.setAttribute('aria-pressed',String(c===choice));});
    picker.querySelector('.app-icon').replaceWith(choice.querySelector('.app-icon').cloneNode(true));
    picker.querySelector('.app-label b').textContent=choice.querySelector('.app-label b').textContent;
    picker.setAttribute('aria-label',text('Выбрать приложение. Сейчас ','Choose an app. Current app: ')+choice.querySelector('.app-label b').textContent);
    var name=choice.querySelector('.app-label b').textContent;
    var installHelp=document.querySelector('[data-help-install]'),importHelp=document.querySelector('[data-help-import]');
    if(installHelp)installHelp.textContent=app.querySelector('.install')?text('Нажмите «Установить '+name+'» на главном экране и установите приложение со страницы загрузки.','Tap “Install '+name+'” on the main screen and install the app from the download page.'):text('Установите '+name+' из официального источника. Если нужна ссылка на загрузку, обратитесь в поддержку.','Install '+name+' from its official source. Ask support if you need the download link.');
    if(importHelp)importHelp.textContent=app.querySelector('[data-import]')?text('Вернитесь сюда и нажмите «Добавить подписку». Если приложение не открылось, скопируйте ссылку в «Ссылка и QR» и вставьте её в разделе импорта приложения.','Return here and tap “Add subscription”. If the app does not open, copy the link from “Link and QR” and paste it into the app’s import screen.'):text('Нажмите «Скопировать ссылку» на главном экране, откройте импорт из буфера обмена в '+name+' и подтвердите добавление профиля.','Tap “Copy link” on the main screen, import from the clipboard in '+name+', and confirm adding the profile.');
    var guide=document.getElementById('selectedGuide');
    if(!guide){guide=document.createElement('div');guide.id='selectedGuide';guide.className='selected-guide';document.querySelector('#helpSheet .sheet-body').prepend(guide);}
    guide.replaceChildren(app.querySelector('.app-help').content.cloneNode(true));
    var instructions=guide.querySelector('details');if(instructions)instructions.open=true;
  }
  function changeDevice(platform){
    var group=applications.filter(function(a){return a.dataset.platform===platform;});
    if(!group.length)return;
    choices.forEach(function(c){c.hidden=c.dataset.platform!==platform;});
    var title=document.getElementById('appsPlatform');if(title)title.textContent=select.options[select.selectedIndex].textContent;
    var app=group.find(function(a){return a.dataset.app===remembered[platform];})||group.find(function(a){return a.dataset.recommended==='true';})||group[0];
    chooseApp(app.dataset.app);
  }
  choices.forEach(function(choice){choice.addEventListener('click',function(){chooseApp(choice.dataset.choice);document.getElementById('appsSheet').close();picker.focus();});});
  if(select){
    var ua=navigator.userAgent;
    var guess=/iPhone|iPad|iPod/i.test(ua)||(/Macintosh/i.test(ua)&&navigator.maxTouchPoints>1)?'ios':/Android/i.test(ua)?'android':/Macintosh|Mac OS X/i.test(ua)?'macos':/Windows/i.test(ua)?'windows':/Linux/i.test(ua)?'linux':null;
    if(Array.from(select.options).some(function(o){return o.value===guess;}))select.value=guess;
    changeDevice(select.value);
    select.addEventListener('change',function(){changeDevice(select.value);});
  }
  function copyLink(event){
    var button=event.currentTarget;
    function markCopied(){button.classList.add('copied');button.innerHTML='<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round"><path d="m5 12 4 4L19 6"/></svg><span>'+text('Ссылка скопирована','Link copied')+'</span>';}

    var input=document.getElementById('subscriptionLink');if(!input)return;
    function fallback(){
      openSheet('linkSheet');input.focus();input.select();
      var copied=false;try{copied=document.execCommand('copy');}catch(_){}
      var result=document.getElementById('copyResult');
      if(!result){result=document.createElement('p');result.id='copyResult';result.className='privacy-note';result.setAttribute('role','status');input.after(result);}
      result.textContent=copied?text('Ссылка скопирована.','Link copied.'):text('Ссылка выделена. Удерживайте её и выберите «Скопировать».','Link selected. Press and hold it, then choose “Copy”.');if(copied)markCopied();
    }
    if(navigator.clipboard&&navigator.clipboard.writeText){navigator.clipboard.writeText(input.value).then(function(){markCopied();
      var sheet=document.getElementById('linkSheet');
      if(sheet.open){var result=document.getElementById('copyResult');if(!result){result=document.createElement('p');result.id='copyResult';result.className='privacy-note';result.setAttribute('role','status');input.after(result);}result.textContent=text('Ссылка скопирована.','Link copied.');}
      else notify(text('Ссылка скопирована. Добавьте её в VPN-приложение.','Link copied. Add it to your VPN app.'));
    },fallback);}else fallback();
  }
  document.querySelectorAll('[data-copy]').forEach(function(button){button.addEventListener('click',copyLink);});
  document.querySelectorAll('[data-import]').forEach(function(button){button.addEventListener('click',function(){notify(text('Подтвердите добавление подписки в приложении.','Confirm adding the subscription in your app.'));});});
})();
