#!/usr/bin/env python3
"""Run PostgreSQL tools without a database password in process arguments."""
import os, sys
from urllib.parse import urlsplit, unquote, parse_qs

def pg_env(url):
    u=urlsplit(url)
    if u.scheme not in ('postgres', 'postgresql') or not u.hostname or not u.path.lstrip('/'):
        raise ValueError('Invalid DATABASE_URL')
    env={'PGHOST':u.hostname,'PGPORT':str(u.port or 5432),'PGDATABASE':unquote(u.path.lstrip('/')),
         'PGUSER':unquote(u.username or ''),'PGPASSWORD':unquote(u.password or '')}
    names={'sslmode':'PGSSLMODE','sslrootcert':'PGSSLROOTCERT','sslcert':'PGSSLCERT','sslkey':'PGSSLKEY',
           'connect_timeout':'PGCONNECT_TIMEOUT','application_name':'PGAPPNAME'}
    for k,v in parse_qs(u.query).items():
        if k not in names: raise ValueError('Unsupported database URL option: '+k)
        env[names[k]]=v[-1]
    return env

if __name__=='__main__':
    try:
        env=os.environ.copy(); env.update(pg_env(env['DATABASE_URL'])); env['SN_PG_ENV_READY']='1'
        os.execvpe(sys.argv[1],sys.argv[1:],env)
    except (ValueError, KeyError, IndexError) as error:
        sys.exit(str(error))
