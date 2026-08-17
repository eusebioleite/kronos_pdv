--drop table cards;

create table cards (
   pedido     varchar2(40) not null,
   status     varchar2(20) default 'NOVO' not null,
   retries    number default 0 not null,
   last_error varchar2(32767),
   created_at date default sysdate not null,
   updated_at date default sysdate not null,
   constraint pk_cards primary key ( pedido )
);

create or replace trigger trg_cards_updated_at before
   update on cards
   for each row
begin
   :new.updated_at := sysdate;
end;

comment on table cards is
   'Fila de integração do Kronos_PDV, automação responsável por transformar pedidos de venda do Debx em cards do DealerCRM..';

comment on column cards.pedido is
   'Código do pedido no Debx.';

comment on column cards.status is
   'Estado da sincronização: ATUALIZAR, NOVO, EXCLUIR, TRAVADO, SUCESSO, EXCLUIDO.';

comment on column cards.retries is
   'Contador de tentativas de sincronizar, máximo de 5.';

comment on column cards.last_error is
   'Ultimo erro de sincronização.';

comment on column cards.created_at is
   'Timestamp de criação.';

comment on column cards.updated_at is
   'Timestamp da última modificação.';

create index idx_cards_status on
   cards (
      status
   );
 
grant all privileges on inventario.cards to kronos;